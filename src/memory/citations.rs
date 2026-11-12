//! Citation syntax and source status, computed only for the night's memory list.

use super::{Controller, Memory, Project, files};
use crate::protocol::{AgentDetail, SessionId};
use std::path::Path;

pub(super) fn valid(cite: &str) -> bool {
    if cite.len() > 500 || cite.contains(['\n', '\r']) {
        return false;
    }
    match cite.split_once(':') {
        Some(("razor", rest)) => rest.rsplit_once(":l-").is_some_and(|(s, n)| {
            SessionId::try_from(s.to_owned()).is_ok() && n.parse::<u64>().is_ok()
        }),
        Some(("file", rest)) => rest.rsplit_once('@').is_some_and(|(p, c)| {
            !p.is_empty() && (7..=64).contains(&c.len()) && c.bytes().all(|c| c.is_ascii_hexdigit())
        }),
        Some(("ref", rest)) => !rest.is_empty(),
        Some(("url", rest)) => rest.rsplit_once(' ').is_some_and(|(u, d)| {
            (u.starts_with("https://") || u.starts_with("http://"))
                && d.parse::<jiff::civil::Date>().is_ok()
        }),
        Some(("task", rest)) => rest.parse::<u32>().is_ok_and(|n| n > 0),
        _ => false,
    }
}

impl Memory {
    /// The memories whose code is no longer simply on main, and references missing on disk.
    pub(crate) async fn citations(&self, c: &Controller, cwd: &Path, all: bool) -> String {
        let scan = move |mut p: Project| {
            if !all {
                p.memory.current.truncate(1);
            }
            // The resident runs this pass, so its own layer's citations are checked too.
            let own = p.resident.map(|r| p.memory.layer(Some(r.as_str())));
            let mut found = Vec::new();
            for memory in std::iter::once(&p.memory).chain(&own) {
                found.extend(files::best(memory, 200, |_| true, |_| Some(0))?.0);
            }
            let cites = found
                .into_iter()
                .flat_map(|f| {
                    let cites = files::head(&f.file).filter(|(k, _)| *k == "cites");
                    cites
                        .map(|(_, v)| (f.id.clone(), v.to_owned()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            Ok((p.root, cites))
        };
        let Ok((root, cites)) = self.files(c, cwd, scan).await else {
            return String::new();
        };
        let mut lines = Vec::new();
        for (id, cite) in cites {
            let status = if let Some(rest) = cite.strip_prefix("file:") {
                let Some((path, commit)) = rest.rsplit_once('@') else {
                    continue;
                };
                code(c, &root, path, commit).await
            } else if let Some(path) = cite.strip_prefix("ref:") {
                if tokio::fs::try_exists(root.join(path))
                    .await
                    .unwrap_or(false)
                {
                    continue;
                }
                "missing reference".into()
            } else {
                continue;
            };
            if status != "on main" {
                lines.push(format!("- {id}: {cite}: {status}"));
            }
        }
        lines.join("\n")
    }
}

async fn code(c: &Controller, root: &Path, path: &str, commit: &str) -> String {
    let git = async |args: &[&str]| crate::children::git::timed(root, 10, args).await;
    if git(&["merge-base", "--is-ancestor", commit, "refs/heads/main"])
        .await
        .is_ok()
    {
        return if git(&["diff", "--quiet", commit, "refs/heads/main", "--", path])
            .await
            .is_ok()
        {
            "on main"
        } else {
            "changed since"
        }
        .into();
    }
    let branches = git(&["branch", "--contains", commit, "--format=%(refname:short)"]).await;
    for branch in branches.unwrap_or_default().lines() {
        if let Some(id) = branch.strip_prefix("child/")
            && let Ok(id) = SessionId::try_from(id.to_owned())
            && settled(c, &id).await
        {
            continue;
        }
        return format!("awaiting merge ({branch})");
    }
    "discarded".into()
}

/// A child branch stays after discard; its parent's summary, local or paired, decides.
async fn settled(c: &Controller, id: &SessionId) -> bool {
    let Ok(child) = c.summary(id).await else {
        return false;
    };
    let Some(child) = child.child else {
        return false;
    };
    if let Some((parent, server)) = child.parent.address() {
        let Some(peer) = c.devices().peer(server).await else {
            return false;
        };
        let path = format!("/v1/sessions/{parent}/agent");
        let detail: Option<AgentDetail> = c.devices().fetch(&peer, &path, &(), 1 << 20).await;
        return detail.is_some_and(|d| {
            d.children.iter().any(|ch| {
                let native = ch
                    .session
                    .address()
                    .map_or_else(|| ch.session.clone(), |(id, _)| id);
                native == *id && ch.settled.is_some()
            })
        });
    }
    c.summary(&child.parent).await.is_ok_and(|p| {
        p.children
            .iter()
            .any(|ch| ch.session == *id && ch.settled.is_some())
    })
}
