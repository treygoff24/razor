//! The nightly pass and its run-now entry, using the same fresh-run lifecycle as schedules.
use super::*;
use crate::config::DreamNight;
use serde_json::{Value, json};

impl Residents {
    pub(super) async fn dreams(&self, due: Millis) {
        let _night = self.inner.dreaming.lock().await;
        let c = &self.inner.controller;
        let Ok(projects) = c.memory().dream_sessions(c, None).await else {
            return;
        };
        let mut all = true;
        for project in projects {
            let root = project.root.clone();
            match self.dream(project, Some(due), &mut all).await {
                Ok(answer) => {
                    let id = answer["session"].as_str();
                    if let Some(id) = id.and_then(|s| SessionId::try_from(s.to_owned()).ok()) {
                        self.closed(&id).await;
                    }
                }
                Err(e) => eprintln!("razor: dreaming in {}: {e}", root.display()),
            }
        }
    }

    /// Returns once session `id` has closed.
    async fn closed(&self, id: &SessionId) {
        let c = &self.inner.controller;
        // Snapshot plus updates has no gap, including a run that closed before subscription.
        let mut status = c.status();
        while let Some(line) = status.recv().await {
            let changed = match line {
                crate::protocol::Live::Agent(row) => row.session == *id,
                crate::protocol::Live::Agents { .. } => true,
                _ => false,
            };
            if changed && c.summary(id).await.is_ok_and(|s| s.closed) {
                break;
            }
        }
    }

    pub(super) async fn dream_now(&self, cwd: &Path) -> Result<Value, Error> {
        let c = &self.inner.controller;
        let project = c
            .memory()
            .dream_project(c, cwd)
            .await
            .map_err(other("reading dreaming's project"))?;
        let mut projects = c
            .memory()
            .dream_sessions(c, Some(project))
            .await
            .map_err(other("listing dreaming's sessions"))?;
        self.dream(projects.remove(0), None, &mut false).await
    }

    async fn dream(
        &self,
        p: crate::memory::Dream,
        due: Option<Millis>,
        all: &mut bool,
    ) -> Result<Value, Error> {
        let c = &self.inner.controller;
        let config = self.config().await;
        if matches!(config.memory.night, DreamNight::Off(_)) {
            return Ok(json!({"off": true}));
        }
        if let Some(run) = &p.run
            && c.summary(run).await.is_ok_and(|s| !s.closed)
        {
            return Ok(json!({"session": run}));
        }
        if due.is_some_and(|d| d <= p.claimed) {
            return Ok(json!({"already_run": true}));
        }
        // The pass reads the person's other servers too. A night is left to a server that will
        // run it as the project's resident, since only that pass tidies the resident's own layer.
        let nights = c.memory().dream_peers(c, &p).await;
        if due.is_some() && p.resident.is_none() && nights.iter().any(|(_, n)| n.resident) {
            return Ok(json!({"deferred": true}));
        }
        let there = nights.iter().flat_map(|(_, n)| &n.sessions);
        let mut sessions: Vec<_> = p.sessions.iter().chain(there).collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.0));
        let text = c.memory().dream_input(c, &p.root, *all).await;
        let ids = sessions
            .iter()
            .map(|(_, id)| format!("- {id}\n"))
            .collect::<String>();
        let mut text = format!(
            "{text}\n\n{}\n{}",
            crate::memory::dream_sessions_heading(),
            ids
        );
        const INPUT_BYTES: usize = 64 << 10; // The schedule-input limit, including every id.
        let room = INPUT_BYTES.checked_sub(text.len() + 128).ok_or_else(|| {
            Error::Refused(
                "Dreaming's instructions and session ids exceed 64 KiB; shorten the instructions."
                    .into(),
            )
        })?;
        let count = sessions.len().min(room / 256);
        let bytes = (room / count.max(1)).min(24 << 10);
        if count < sessions.len() {
            text.push_str("\nOldest sessions have ids alone; recall each one.\n");
        }
        let mut unread = std::collections::BTreeSet::new();
        for (index, (_, id)) in sessions.iter().enumerate() {
            if index >= count {
                unread.insert(id.address().map(|(_, name)| name));
                continue;
            }
            match crate::children::digest(c, id, bytes.saturating_sub(32)).await {
                Ok(digest) => text.push_str(&format!("\n- {id}\n{digest}")),
                // Recall may recover it, but the server cannot certify that; keep its read mark.
                Err(_) if id.address().is_some() => {
                    unread.insert(id.address().map(|(_, name)| name));
                }
                Err(e) => return Err(other("reading a dreaming digest")(e)),
            }
        }
        if due.is_some() && sessions.is_empty() {
            return Ok(json!({"inactive": true}));
        }
        let at = now();
        let id = SessionId::new();
        let covered = nights.iter().map(|(_, n)| n.server.clone()).collect();
        if !c
            .memory()
            .dream_stamp(c, &p, &id, covered)
            .await
            .map_err(other("claiming dreaming"))?
        {
            return Ok(json!({"already_run": true}));
        }
        let model = config.memory.model.map(crate::profiles::id);
        let session = if let Some(resident) = &p.resident {
            // Dreaming uses the server defaults, rather than a resident's own model override.
            let schedule = Schedule {
                at: Vec::new(),
                days: Vec::new(),
                text,
                model: Some(model.unwrap_or(config.model)),
                effort: Some(config.effort),
                deliver: Deliver::None,
                fresh: true,
            };
            self.schedule(resident, "dream", &schedule, at, Some(id))
                .await?
                .1
        } else {
            let model = Some(model.unwrap_or(config.model));
            let id = c
                .create(
                    p.root.clone(),
                    model,
                    (Some(config.effort), None),
                    Some(id),
                    None,
                    None,
                )
                .await
                .map_err(other("starting dreaming"))?;
            let name = format!("dream · {}", jiff::Zoned::now().date());
            c.append(&id, Body::Name { name })
                .await
                .map_err(other("naming dreaming"))?;
            let from = Origin::Schedule {
                name: "dream".into(),
                due: at,
            };
            c.admit(&id, Input::new(from, text))
                .await
                .map_err(other("admitting dreaming"))?;
            id
        };
        *all = false;
        // Only a pass that ended has read its sessions; a crash leaves them for the next night.
        let read = nights
            .iter()
            .filter(|(name, _)| !unread.contains(&Some(name.as_str())))
            .map(|(_, n)| (n.server.clone(), n.through));
        let read = read
            .chain((!unread.contains(&None)).then(|| (p.server.clone(), p.through)))
            .collect();
        let (residents, ended) = (self.clone(), session.clone());
        tokio::spawn(async move {
            residents.closed(&ended).await;
            let c = &residents.inner.controller;
            if let Err(e) = c.memory().dream_read(c, &p, read).await {
                eprintln!("razor: marking dreaming's sessions read: {e}");
            }
        });
        Ok(json!({"session": session}))
    }
}
