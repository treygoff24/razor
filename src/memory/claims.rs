//! Claims as sessions end (docs/memory.md): a parent claims a task for the child it starts,
//! a session's claims are released when it closes or is settled, a settled child's report is a
//! comment on each, and the tick syncs shared projects waiting on a task, reminds of overdue tasks
//! and releases what a failed release left held.

use super::tasks::{self, Edit};
use super::{Answer, By, Memory, Note, Project, TEXT, blocking, fill, open, send};
use crate::controller::Controller;
use crate::protocol::{Decision, ReportStatus, SessionId};
use crate::record::{ChildSummary, Summary};
use serde_json::Value;
use std::fmt;
use std::path::Path;
use std::sync::atomic::Ordering::Relaxed;

impl Memory {
    /// `s` closed with `/done` or `/quit`: a root's claims on this server are released on a task
    /// of their own, which never fails the close. A child's wait for its settle.
    pub(crate) fn closed(&self, c: &Controller, s: &Summary) {
        if s.child.is_none() {
            let (c, s) = (c.clone(), s.clone());
            tokio::spawn(async move {
                let by = (&s.session, s.name.as_str());
                c.memory().release(&c, &s.cwd, by, None, None).await
            });
        }
    }

    /// Claims task `id` for the child `by`, as its parent, `s`, starts it: allowed when the task
    /// is unclaimed or the parent's. A refusal leaves the child running.
    pub(crate) async fn start(&self, c: &Controller, s: &Summary, by: By<'_>, id: u32) -> Answer {
        let mut edit = Edit::default();
        (edit.id, edit.claim, edit.from) = (Some(id), Some(true), Some(s.session.to_string()));
        self.edit(c, &s.cwd, by, edit).await
    }

    /// The child `child` of `parent` was settled by decision `how`: its report becomes a comment
    /// on each task it holds. A merge of a finished child closes only the task named at its
    /// start, with the report's first line; every other claim is released, open.
    pub(crate) async fn settled(
        &self,
        c: &Controller,
        parent: &Summary,
        child: &ChildSummary,
        how: Decision,
    ) {
        let by = (&child.session, child.child.name.as_str());
        let mut report = None;
        if let Some(r) = &child.report {
            // The summary keeps a report's first line; its entry in the parent's record, all of it.
            let entry = c.get(&parent.session, r.entry).await.unwrap_or_default();
            let entry = serde_json::from_str::<Value>(&entry).unwrap_or_default();
            let text = entry["text"].as_str().unwrap_or(&r.text).to_owned();
            let mut close = None;
            if how != Decision::Discard && r.status == ReportStatus::Finished {
                let Ok(start) = c.get(&parent.session, child.child.launched).await else {
                    self.owed.store(true, Relaxed);
                    return;
                };
                let start = serde_json::from_str::<Value>(&start).unwrap_or_default();
                close = start["input"]["task"]
                    .as_u64()
                    .and_then(|n| n.try_into().ok());
            }
            report = Some((text, close));
        }
        let report = report.as_ref().map(|(text, close)| (text.as_str(), *close));
        self.release(c, &parent.cwd, by, report, Some(parent)).await;
        if child.session.address().is_none() {
            let Ok(s) = c.summary(&child.session).await else {
                self.owed.store(true, Relaxed);
                return;
            };
            let worktree = s.child.as_ref().and_then(|child| child.worktree.as_ref());
            // A retired ancestor may have removed the origin while this checkout survives.
            let cwd = if tokio::fs::try_exists(&s.cwd).await.unwrap_or(false) {
                &s.cwd
            } else {
                worktree.map_or(&s.cwd, |w| &w.origin)
            };
            if s.children.iter().any(|child| child.settled.is_none()) {
                self.owed.store(true, Relaxed);
            }
            if *cwd != parent.cwd {
                self.release(c, cwd, by, report.map(|(text, _)| (text, None)), None)
                    .await;
            }
        }
    }

    /// From the residents' wake, at most once a minute, on a task of its own and not while a
    /// change or the last tick holds the lock: each shared project with a task held here that
    /// waits on an open task syncs, so a close made on another machine reaches it; each overdue
    /// task reminds its open claimant on this server, else its open creator, once for its due
    /// date; and after a release failed, or first after a start, a closed or settled session's
    /// claims here are released.
    pub(crate) fn tick(&self, c: &Controller) {
        let c = c.clone();
        tokio::spawn(async move {
            let memory = c.memory();
            let Ok(held) = memory.lock.try_lock() else {
                return;
            };
            let (notes, holders) = memory.visit(&c).await;
            drop(held);
            send(&c, notes).await;
            for id in holders {
                memory.recover(&c, &id).await;
            }
        });
    }

    /// The tick's work, under the lock: the notes, and when releases may be owed, the sessions
    /// holding tasks here.
    async fn visit(&self, c: &Controller) -> (Vec<Note>, Vec<String>) {
        let dir = self.home.join("tasks");
        let found = blocking(move || Ok(tasks::projects(&dir))).await;
        let (mut notes, mut holders, today) = (Vec::new(), Vec::new(), jiff::Zoned::now().date());
        let (today, owed) = (today.to_string(), self.owed.swap(false, Relaxed));
        // A folder unread keeps owed releases owed; with none owed, nothing was missed.
        let (projects, whole) = found.unwrap_or_default();
        if owed && !whole {
            self.owed.store(true, Relaxed);
        }
        for project in projects {
            let read = |p: Project| Ok((tasks::scan(&p.tasks, false)?, p));
            let Ok((all, p)) = self.files(c, &project, read).await else {
                if owed {
                    self.owed.store(true, Relaxed);
                }
                continue;
            };
            let held = all.iter().filter_map(|t| t.holder(&p.server));
            holders.extend(held.filter(|_| owed).map(String::from));
            let live = |n| tasks::open(&all, n);
            let waits = |t: &tasks::Task| t.holder(&p.server).is_some() && t.blockers().any(live);
            if matches!(p.remote, Ok(Some(_))) && all.iter().any(waits) {
                notes.extend(self.locked(c, &p, None).await.1);
            }
            let unsent = |t: &&tasks::Task| t.get("notified") != t.get("due");
            let due = |t: &&tasks::Task| unsent(t) && t.get("due").is_some_and(|d| *d < *today);
            for t in all.iter().filter(due) {
                // The open claimant on this server, else the open creator.
                let (mut heard, lines) = (None, [t.get("claim"), t.get("by")].into_iter());
                for line in lines.flatten().filter(|l| tasks::ids(l).1 == p.server) {
                    heard = heard.or(open(c, tasks::ids(line).0).await.map(|s| (s, line)));
                }
                let Some((session, line)) = heard else {
                    continue;
                };
                // Sent only if the task's header is as read, after a fetch and on every retry.
                let (mut edit, due) = (Edit::default(), t.get("due").unwrap_or_default());
                (edit.id, edit.notified) = (Some(t.id), Some(due.into()));
                edit.seen = Some(t.head.clone());
                let (done, more) = self.locked(c, &p, Some((line.into(), edit))).await;
                let title = t.get("title").unwrap_or_default();
                let said: [(_, &dyn fmt::Display); 3] =
                    [("id", &t.id), ("title", &title), ("due", &due)];
                if done.is_ok() {
                    notes.push((session.to_string(), fill(&TEXT.overdue, &said)));
                }
                notes.extend(more);
            }
        }
        (notes, holders)
    }

    /// The claims session `id` holds here, released as its close or its settle would have
    /// released them, had that release not failed: a root's once it is closed, a child's once
    /// it or an ancestor is settled, or its root is closed. A report alone releases nothing.
    async fn recover(&self, c: &Controller, id: &str) -> Option<()> {
        let read = async |id: &SessionId| {
            let summary = c.summary(id).await;
            // A session gone for good has nothing left to retry.
            if summary
                .as_ref()
                .is_err_and(|e| !matches!(e, crate::controller::Error::NoSession(_)))
            {
                self.owed.store(true, Relaxed);
            }
            summary.ok()
        };
        let id = SessionId::try_from(id.to_owned()).ok()?;
        let s = read(&id).await?;
        if s.closed && s.child.is_none() {
            self.release(c, &s.cwd, (&id, &s.name), None, None).await;
        }
        let parent = read(&s.child?.parent).await?;
        let child = parent.children.iter().find(|k| k.session == id)?;
        let mut how = child.settled;
        let mut ancestor = parent.clone();
        // At most 64 ancestors per recovery: malformed parent links cannot loop forever.
        for _ in 0..64 {
            if how.is_some() || ancestor.child.is_none() {
                how = how.or(ancestor.closed.then_some(Decision::Discard));
                break;
            }
            let above = read(&ancestor.child.as_ref()?.parent).await?;
            let settled = above
                .children
                .iter()
                .any(|k| k.session == ancestor.session && k.settled.is_some());
            how = settled.then_some(Decision::Discard);
            ancestor = above;
        }
        let how = how?;
        self.settled(c, &parent, child, how).await;
        Some(())
    }

    /// Releases each task session `by.0` holds in `cwd`'s project on this server, with
    /// `report.0` as a comment on it, closing only `report.1` with its first line; a task
    /// whose header changed meanwhile, its claim perhaps moved, is left as it is.
    async fn release(
        &self,
        c: &Controller,
        cwd: &Path,
        by: By<'_>,
        report: Option<(&str, Option<u32>)>,
        closer: Option<&Summary>,
    ) {
        let me = by.0.to_string();
        let held = self.files(c, cwd, move |p| {
            let all = tasks::scan(&p.tasks, false)?;
            let held = all.into_iter().filter(|t| t.holder(&p.server) == Some(&me));
            Ok((held.map(|t| (t.id, t.head)).collect::<Vec<_>>(), p.server))
        });
        let report = report.filter(|r| !r.0.trim().is_empty());
        let comment = report.map(|(text, _)| text[..text.floor_char_boundary(16 << 10)].to_owned());
        let (held, server) = match held.await {
            Ok(held) => held,
            Err(e) => {
                self.owed.store(true, Relaxed);
                eprintln!("razor: finding tasks of {}: {e}", by.0);
                return;
            }
        };
        for (id, seen) in held {
            let close = report
                .filter(|r| r.1 == Some(id))
                .and_then(|r| r.0.trim_start().lines().next());
            let close = close.map(|line| line[..line.floor_char_boundary(500)].to_owned());
            let mut edit = Edit::default();
            (edit.id, edit.claim, edit.comment, edit.close) =
                (Some(id), Some(false), comment.clone(), close.clone());
            edit.seen = Some(seen);
            edit.closer = closer.map(|s| super::who(&s.session, &server, &s.name));
            if let Err(e) = self.edit(c, cwd, by, edit).await {
                eprintln!("razor: releasing task {id} of {}: {e}", by.0);
                self.owed.store(true, Relaxed);
            }
        }
    }
}
