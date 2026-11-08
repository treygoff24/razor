//! Controller operations that reach a session only through its queue or its directory.

use super::*;

/// How long after its record last changed an unlisted root keeps its name from a new root.
const WEEK: Duration = Duration::from_secs(7 * 86_400);
/// How long after a person's input an idle root stays live in the agents list.
const RECENT: Duration = Duration::from_secs(12 * 3600);
/// A resident's newest threads the agents list can show: the open ones and a finished line.
const THREADS: usize = 12;
/// The most bytes a session's draft holds.
const DRAFT: usize = 1 << 20;

impl Controller {
    /// The home whose sessions this controller runs.
    pub(crate) fn home(&self) -> &Path {
        &self.inner.home
    }

    /// Writes a new session's Start, or reopens a named session that is not a child; a child
    /// never reuses an id, and its parent's `Started` is the caller's to record. A root Razor
    /// starts (no id, no child) is named `<server.name>-<project>`, or that with the lowest
    /// `-2`, `-3` … that no listed agent, and no root whose record changed in the last week, is
    /// named; it is stored in its Start. A resident's thread is a root whose Start names it, and
    /// is named the same way `<resident>-<noun>`, the noun its id's word name ends in.
    pub(crate) async fn create(
        &self,
        cwd: PathBuf,
        model: Option<String>,
        choice: (Option<Effort>, Option<Speed>),
        id: Option<SessionId>,
        child: Option<Child>,
        thread: Option<Thread>,
    ) -> Result<SessionId, Error> {
        let settings = self.machine_settings().await;
        let root = child
            .as_ref()
            .and_then(|c| c.worktree.as_ref())
            .map_or(cwd.as_path(), |w| w.origin.as_path());
        let config = if child.is_some() && !tokio::fs::try_exists(root).await? {
            settings.config
        } else {
            settings.project(root)?.config
        };
        let model = model.unwrap_or_else(|| config.model.clone());
        let actions = config.web.actions().collect::<Vec<_>>().into();
        let home = &self.inner.home;
        let version = config
            .profile(home, &model, actions, child.is_some().into())?
            .version;
        let effort = choice.0.unwrap_or(config.effort);
        let speed = choice.1.unwrap_or(if child.is_some() {
            Speed::Standard
        } else {
            config.speed
        });
        crate::profiles::speed(&model, speed, &config)?;
        let _naming = self.inner.naming.lock().await;
        let mut session = id.clone().unwrap_or_else(SessionId::new);
        // A thread keeps this name until the person or its resident renames it; a fresh run's job
        // renames its thread at once.
        let name = match (&id, &child) {
            (None, None) => {
                let roots = record::list(&self.inner.home, None, None, false, usize::MAX).await?;
                let held = roots.iter().filter(|(.., name, at)| {
                    !name.is_empty() && !at.elapsed().is_ok_and(|age| age >= WEEK)
                });
                let status = lock(&self.inner.status);
                let mut taken: Vec<&str> =
                    status.agents.values().map(|r| r.name.as_str()).collect();
                taken.extend(held.map(|(.., name, _)| name.as_str()));
                let (project, word) = (cwd.file_name().unwrap_or_default(), session.name());
                let (first, second) = match &thread {
                    Some(t) => (t.resident.as_str(), word.rsplit('-').next().unwrap_or("")),
                    None => (config.server.name.as_str(), &*project.to_string_lossy()),
                };
                children::name(first, second, &taken)
            }
            _ => String::new(),
        };
        loop {
            match Record::create(
                &self.inner.home,
                &session,
                cwd.clone(),
                model.clone(),
                ModelSettings { effort, speed },
                version.clone(),
                name.clone(),
                child.clone(),
                thread.clone(),
            )
            .await
            {
                Ok(record) => {
                    // A child's Start precedes its checkout; load its settings only at admission,
                    // after Children has made that folder and its linked-worktree pointer.
                    if child.is_none() {
                        self.task(&session, Some(record));
                    }
                    break;
                }
                Err(record::Error::Exists(_)) if id.is_none() => session = SessionId::new(),
                Err(record::Error::Exists(_)) if child.is_none() => break,
                Err(e) => return Err(e.into()),
            }
        }
        self.seed(&session).await?;
        Ok(session)
    }

    /// The loaded fold, or a read-only snapshot, resumed for configured resident homes;
    /// never starts a session task.
    pub(crate) async fn summary(&self, session: &SessionId) -> Result<Summary, Error> {
        if let Some(summary) = self.loaded_summary(session).await? {
            return Ok(summary);
        }
        let summary = self
            .inner
            .summaries
            .read(&self.inner.home, session)
            .await
            .map_err(|e| {
                if matches!(&e, record::Error::Missing(_)) {
                    self.forget(session);
                }
                Error::from(e)
            })?;
        Ok(summary)
    }

    /// Reads only an already loaded task, holding its sender until the queued read completes.
    pub(crate) async fn loaded_summary(
        &self,
        session: &SessionId,
    ) -> Result<Option<Summary>, Error> {
        let task = lock(&self.inner.sessions).get(session).cloned();
        let Some(_task) = task else { return Ok(None) };
        self.ask(session, |s| {
            Box::pin(async move { Ok(Some(s.record.summary().clone())) })
        })
        .await
    }

    /// Admits a display row at creation and restart seeding, never from a historical read.
    async fn seed(&self, session: &SessionId) -> Result<(), Error> {
        let summary = self.summary(session).await?;
        let row = self.row(&summary).await;
        let mut status = lock(&self.inner.status);
        status.row(row);
        status.child_rows(&summary);
        Ok(())
    }

    /// Seeds missing local display rows without loading session tasks.
    pub(crate) async fn admit_rows(&self, ids: impl IntoIterator<Item = SessionId>) {
        for id in ids {
            if id.address().is_none() && !lock(&self.inner.status).agents.contains_key(&id) {
                let _ = self.seed(&id).await;
            }
        }
    }

    /// Silently drops only candidates with no loaded task and no terminal.
    pub(crate) fn evict<'a>(&self, candidates: impl Iterator<Item = &'a AgentView>) {
        let sessions = lock(&self.inner.sessions);
        let mut status = lock(&self.inner.status);
        for row in candidates {
            if !sessions.contains_key(&row.session)
                && row.terminals == 0
                && status.agents.get(&row.session) == Some(row)
            {
                status.agents.remove(&row.session);
            }
        }
    }

    async fn row(&self, summary: &Summary) -> AgentView {
        if let Some(row) = lock(&self.inner.status)
            .agents
            .get(&summary.session)
            .cloned()
        {
            return row;
        }
        let config = self.inner.config.lock().await;
        let limits = crate::profiles::limits(&summary.model, &config.config, summary.window);
        let state = [State::NeedsYou, State::Done][usize::from(summary.done())];
        lock(&self.inner.status).build(summary, state, None, false, limits)
    }

    /// One session's display row, without inserting it into the live status map.
    pub(crate) async fn agent(&self, session: &SessionId) -> Result<AgentView, Error> {
        let mut row = self.row(&self.summary(session).await?).await;
        self.parent_row(&mut row).await?;
        Ok(row)
    }

    /// All direct children, including settled helpers whose cards remain in the conversation.
    pub(crate) async fn detail(
        &self,
        session: &SessionId,
    ) -> Result<crate::protocol::AgentDetail, Error> {
        let parent = self.summary(session).await?;
        let mut row = self.row(&parent).await;
        self.parent_row(&mut row).await?;
        let mut rows = Vec::new();
        for child in &parent.children {
            let mut row = if child.settled.is_some() || child.session.address().is_some() {
                let cached = lock(&self.inner.status).agents.get(&child.session).cloned();
                if let Some(row) = cached {
                    row
                } else {
                    let mut row = children::child(session, child);
                    if child.session.address().is_none()
                        && let Some(Body::Start {
                            model, settings, ..
                        }) = record::start(self.home(), &child.session).await
                    {
                        row.model = model;
                        row.effort = settings.effort;
                    }
                    row
                }
            } else {
                self.row(&self.summary(&child.session).await?).await
            };
            children::child_row(session, child, &mut row);
            rows.push(row);
        }
        Ok(crate::protocol::AgentDetail {
            row,
            children: rows,
            changed_files: Some(parent.changed_files),
        })
    }

    /// A row read through this server keeps the facts its locally stored parent owns.
    pub(crate) async fn parent_row(&self, row: &mut AgentView) -> Result<(), Error> {
        let Some(parent) = &row.parent else {
            return Ok(());
        };
        let native = |s: &SessionId| s.address().map_or_else(|| s.clone(), |(id, _)| id);
        let parent = match self.summary(&native(parent)).await {
            Ok(parent) => parent,
            Err(Error::NoSession(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        if let Some(fact) = parent
            .children
            .iter()
            .find(|c| native(&c.session) == native(&row.session))
        {
            children::child_row(&parent.session, fact, row);
        }
        Ok(())
    }

    /// The stored entry, read through the record's index.
    pub(crate) async fn get(&self, session: &SessionId, entry: EntryId) -> Result<String, Error> {
        self.ask(session, move |s| {
            Box::pin(async move { Ok(s.record.get(entry).await?) })
        })
        .await
    }

    /// A contiguous range before `before`: at most `PAGE_BYTES`, or one larger entry alone.
    pub(crate) async fn page(
        &self,
        session: &SessionId,
        before: Option<EntryId>,
        limit: usize,
    ) -> Result<Replay, Error> {
        let bytes = crate::protocol::PAGE_BYTES;
        self.ask(session, move |s| {
            Box::pin(async move { Ok(s.record.page(before, limit, bytes).await?) })
        })
        .await
    }

    /// Records a fact or server question; inputs always use admission.
    pub(crate) async fn append(&self, session: &SessionId, body: Body) -> Result<EntryId, Error> {
        if !matches!(
            body,
            Body::VoiceCall { .. }
                | Body::Child { .. }
                | Body::Headline { .. }
                | Body::ChangedFiles { .. }
                | Body::MailSent { .. }
                | Body::Question { .. }
                | Body::Name { .. }
                | Body::Fresh { .. }
        ) {
            return invalid("Inputs use admission; only fact entries can be appended.");
        }
        self.ask(session, |s| Box::pin(s.append(body, None))).await
    }

    /// The child's own session decides, not its parent's fact: a running turn refuses, an open
    /// child takes the input as it takes a note, and a closed one reopens on it.
    pub(crate) async fn continue_child(
        &self,
        id: &SessionId,
        input: Input,
    ) -> Result<EntryId, Error> {
        self.ask(id, |s| Box::pin(s.take(input, true))).await
    }

    /// Closes the session, all its jobs and its descendants; later inputs are held until
    /// `continue_child` releases it.
    pub(crate) async fn close(&self, session: &SessionId) -> Result<(), Error> {
        let (local, mut failure) = children::descendants(self, session, false).await;
        for id in local {
            if let Err(e) = self.ask(&id, |s| Box::pin(s.close())).await {
                failure = Some(e.into());
            }
        }
        failure.map_or(Ok(()), |e| Err(Error::Invalid(e.to_string())))
    }

    /// Closes the session for good, as a settle does: every input it still holds ends
    /// `Stopped`, since it is never continued again.
    pub(crate) async fn retire(&self, session: &SessionId) -> Result<(), Error> {
        self.ask(session, |s| {
            Box::pin(async move {
                s.close().await?;
                s.close_out(&[]).await
            })
        })
        .await
    }

    /// `/done`: admits Razor's closing input and answers at once. The turn that takes it is the
    /// session's last: when it ends, every input still waiting ends `Stopped` and the session
    /// closes; its next input reopens it. With no `closeout` (`/quit`) the turn ending now is
    /// the last, with no closing turn, and the answer is entry 0.
    pub(crate) async fn done(&self, session: &SessionId, closeout: bool) -> Result<EntryId, Error> {
        self.ask(session, move |s| Box::pin(s.done(closeout))).await
    }

    /// Refuses a model this session cannot run, as admitting an input naming it would.
    pub(crate) async fn choosable(&self, session: &SessionId, model: String) -> Result<(), Error> {
        self.ask(session, move |s| {
            Box::pin(async move { s.choosable(Some(&model), None) })
        })
        .await
    }

    /// Records an effort choice without input or a turn; the next model request uses it.
    pub(crate) async fn effort(
        &self,
        session: &SessionId,
        effort: Effort,
    ) -> Result<Option<EntryId>, Error> {
        self.ask(session, move |s| Box::pin(s.effort(effort))).await
    }

    /// What switching `session` to each of `models` but its own reads without the prompt cache.
    pub(crate) async fn switch(
        &self,
        session: &SessionId,
        models: Vec<String>,
    ) -> Result<BTreeMap<String, crate::protocol::Switch>, Error> {
        self.ask(session, move |s| Box::pin(s.switch(models))).await
    }

    /// Requests the threshold's continuation note, for a rebuild recorded as `handover`; answers
    /// on admission, or `None` if already requested/rebuilding; refuses a closed child.
    pub(crate) async fn handover(&self, session: &SessionId) -> Result<Option<EntryId>, Error> {
        self.ask(session, |s| Box::pin(s.handover(Trigger::Handover)))
            .await
    }

    /// The first news (mail, a child's report or a Razor notice) admitted after the call recorded
    /// at `at`, or `None` at `timeout`; held news is taken at once. Taken news is this call's
    /// result, not model input, and that Result answers it.
    pub(crate) async fn wait_input(
        &self,
        session: &SessionId,
        at: EntryId,
        timeout: Duration,
    ) -> Result<Option<EntryId>, Error> {
        let taken = self
            .ask(session, move |s| Box::pin(async move { s.wait_input(at) }))
            .await?;
        let taken = tokio::time::timeout(timeout, taken).await;
        Ok(taken.ok().and_then(Result::ok))
    }

    /// Answers a still-open recorded question once, in the asker's own record, naming the
    /// enrolled device that answered.
    pub(crate) async fn answer(
        &self,
        session: &SessionId,
        ask: EntryId,
        answer: Answer,
        device: Option<String>,
    ) -> Result<EntryId, Error> {
        self.ask(session, move |s| Box::pin(s.answer(ask, answer, device)))
            .await
    }

    /// Withdraws an input not yet sent to a model; its `Stopped` turn end is durable.
    pub(crate) async fn withdraw(&self, session: &SessionId, entry: EntryId) -> Result<(), Error> {
        self.ask(session, move |s| Box::pin(s.withdraw(entry)))
            .await
    }

    /// A resident's night rebuilds its context: at once when idle, else when the turn its input
    /// `at` opened ends, or once the current batch of a turn that input waits behind settles.
    pub(crate) async fn night(&self, name: &SessionId, at: Option<EntryId>) -> Result<(), Error> {
        self.tell(name, move |s| Box::pin(s.night(at))).await
    }

    /// Stores a JPEG, PNG or WebP image, its type sniffed from its bytes, never the client's
    /// label; no decoded bytes enter a session's queue.
    pub(crate) async fn store(&self, session: &SessionId, bytes: Vec<u8>) -> Result<Image, Error> {
        self.summary(session).await?;
        if bytes.len() > crate::protocol::IMAGE_BYTES {
            return invalid("An image is larger than 7.5 MB; send a smaller image.");
        }
        let Some((media_type, extension)) = crate::protocol::image_type(&bytes) else {
            return invalid("Send a JPEG, PNG or WebP image.");
        };
        let path = format!("images/{:016x}.{extension}", crate::protocol::random());
        self.keep(session, &path, bytes).await?;
        Ok(Image {
            media_type: media_type.into(),
            path: path.into(),
        })
    }

    /// Writes `bytes` to `name` inside the session's directory in one rename, making its folders
    /// and never following a link: a person's upload, or a reply's audio.
    pub(crate) async fn keep(
        &self,
        session: &SessionId,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        let (dir, name) = (self.dir(session), name.to_string());
        blocking(move || files::write(&dir, &name, &bytes)).await
    }

    /// A regular file inside the session's directory, validated and opened in one step, never
    /// through a symlink; no path is returned to reopen.
    pub(crate) async fn file(
        &self,
        session: &SessionId,
        name: &str,
    ) -> Result<tokio::fs::File, Error> {
        self.summary(session).await?;
        let (dir, name) = (self.dir(session), name.to_string());
        let folder = dir.clone();
        let file = blocking(move || files::open(&dir, &name)).await;
        if file.is_err() && !tokio::fs::try_exists(folder).await? {
            self.forget(session);
            return Err(Error::NoSession(session.clone()));
        }
        Ok(tokio::fs::File::from_std(file?))
    }

    /// The session's unsent UTF-8 words, at most 1 MiB; an absent draft is empty.
    pub(crate) async fn draft(&self, session: &SessionId) -> Result<String, Error> {
        let file = match self.file(session, "draft").await {
            Err(Error::Other { source, .. })
                if source
                    .downcast_ref::<io::Error>()
                    .is_some_and(|e| e.kind() == io::ErrorKind::NotFound) =>
            {
                return Ok(String::new());
            }
            file => file?,
        };
        let mut text = String::new();
        file.take(DRAFT as u64 + 1)
            .read_to_string(&mut text)
            .await?;
        if text.len() > DRAFT {
            return invalid("The draft is larger than 1 MiB; shorten it.");
        }
        Ok(text)
    }

    /// Replaces the draft atomically, never writing through a symlink, after every write that
    /// reached the draft lock first, an admission's clearing included.
    pub(crate) async fn set_draft(&self, session: &SessionId, text: String) -> Result<(), Error> {
        // Read before the lock: the session's task may be waiting for it in an admission.
        self.summary(session).await?;
        if text.len() > DRAFT {
            return invalid("The draft is larger than 1 MiB; shorten it.");
        }
        self.write_draft(session, text).await
    }

    /// A restart leaves idle sessions unloaded, and a row exists only for a loaded session or a
    /// restart seed. This reads the summaries of the roots whose records changed within `RECENT`
    /// and of each resident's `THREADS` most recently changed threads, whatever their age. The
    /// publisher trims these candidates on its first pass. No session task starts; a stat of
    /// the record decides before any of it is folded.
    pub(super) async fn recent(&self) {
        let roots = record::list(&self.inner.home, None, None, false, usize::MAX).await;
        let mut roots = roots.unwrap_or_default();
        roots.sort_by_key(|root| std::cmp::Reverse(root.4));
        let mut taken = BTreeMap::<SessionId, usize>::new();
        for (session, _, thread, _, changed) in roots {
            let newest = thread.is_some_and(|t| {
                let n = taken.entry(t.resident).or_default();
                *n += 1;
                *n <= THREADS
            });
            if newest || changed.elapsed().is_ok_and(|age| age < RECENT) {
                let _ = self.seed(&session).await;
            }
        }
    }

    /// Roots newest first with their latest recorded model, effort, title and activity; with
    /// `resident`, its threads, and fresh runs too when `jobs`; reading never registers a row.
    pub(crate) async fn list(
        &self,
        cwd: Option<PathBuf>,
        resident: Option<&SessionId>,
        jobs: bool,
        limit: usize,
    ) -> Result<Vec<Listed>, Error> {
        let mut listed = Vec::new();
        let roots = record::list(&self.inner.home, cwd.as_deref(), resident, jobs, limit);
        for (session, ..) in roots.await? {
            let summary = self.summary(&session).await?;
            let repo = children::repo(&summary);
            listed.push(summary.listed(repo));
        }
        Ok(listed)
    }

    /// The latest rows of the tree holding `session`, following only parent/child links, without
    /// settled helpers and their subtrees. Parents precede their children, including rows
    /// supplied by a remote overlay.
    pub(crate) async fn tree(&self, session: &SessionId) -> Result<Vec<AgentView>, Error> {
        let root = children::root(self, session)
            .await
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let records = children::records(self, &root).await;
        if let Some(e) = records.failure {
            return invalid(&e.to_string());
        }
        let mut rows = Vec::new();
        for summary in &records.local {
            rows.push(self.row(summary).await);
        }
        for summary in &records.local {
            for child in &summary.children {
                if let Some(row) = rows.iter_mut().find(|r| r.session == child.session) {
                    children::child_row(&summary.session, child, row);
                }
            }
        }
        rows.extend(records.remote.iter().map(|(p, c)| children::child(p, c)));
        Ok(rows)
    }

    /// Local parents' foreign child facts, copied without opening records.
    pub(crate) fn remote_children(&self) -> Vec<(SessionId, SessionId)> {
        lock(&self.inner.status)
            .remote
            .iter()
            .map(|(child, parent)| (parent.clone(), child.clone()))
            .collect()
    }

    /// Addresses outstanding foreign children's parents through their local records.
    pub(crate) fn parent_rows(&self, rows: &mut [AgentView]) {
        let status = lock(&self.inner.status);
        for row in rows {
            if let Some(parent) = status.remote.get(&row.session) {
                row.parent = Some(parent.clone());
            }
        }
    }

    /// Peer updates validate and admit within one parent queue operation; no relaying.
    pub(crate) async fn receive_child(
        &self,
        server: &str,
        request: ChildRequest,
    ) -> Result<Value, children::Error> {
        self.inner
            .children
            .receive_child(self, server, request)
            .await
    }

    /// Checks and admits one direct child's update in its parent's queue; a report waits for
    /// room as any other input does.
    pub(crate) async fn update_child(
        &self,
        parent: &SessionId,
        child: SessionId,
        update: crate::protocol::ChildUpdate,
    ) -> Result<Value, Error> {
        self.with_room(true, move || {
            let (child, update) = (child.clone(), update.clone());
            self.ask(parent, |s| {
                Box::pin(async move {
                    let Some(update) = children::update(s.record.summary(), &child, update)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                    else {
                        return Ok(json!({}));
                    };
                    let entry = match update {
                        Body::Input(input) => s.admit(input).await?,
                        body => s.append(body, None).await?,
                    };
                    Ok(json!({"entry":entry}))
                })
            })
        })
        .await
    }

    /// Closes and reads the current outgoing report in one queue operation; a new task forbids
    /// both.
    pub(crate) async fn reported(&self, session: &SessionId) -> Result<Option<Input>, Error> {
        self.ask(session, |s| {
            Box::pin(async move {
                let Some(at) = s.record.summary().reported else {
                    return Ok(None);
                };
                s.close().await?;
                Ok(match s.record.entry(at).await?.body {
                    Body::TurnEnd { report, .. } => report.map(|r| *r),
                    _ => None,
                })
            })
        })
        .await
    }
}
