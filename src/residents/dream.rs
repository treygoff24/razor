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
        for project in projects {
            let root = project.root.clone();
            match self.dream(project, Some(due)).await {
                Ok(answer) => {
                    let Some(id) = answer["session"]
                        .as_str()
                        .and_then(|s| SessionId::try_from(s.to_owned()).ok())
                    else {
                        continue;
                    };
                    // Snapshot plus updates has no gap, including a run that closed before subscription.
                    let mut status = c.status();
                    while let Some(line) = status.recv().await {
                        let changed = match line {
                            crate::protocol::Live::Agent(row) => row.session == id,
                            crate::protocol::Live::Agents { .. } => true,
                            _ => false,
                        };
                        if changed && c.summary(&id).await.is_ok_and(|s| s.closed) {
                            break;
                        }
                    }
                }
                Err(e) => eprintln!("razor: dreaming in {}: {e}", root.display()),
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
        self.dream(projects.remove(0), None).await
    }

    async fn dream(&self, p: crate::memory::Dream, due: Option<Millis>) -> Result<Value, Error> {
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
        if due.is_some_and(|d| d <= p.since) {
            return Ok(json!({"already_run": true}));
        }
        let mut lines = Vec::new();
        for id in &p.sessions {
            let Ok(summary) = c.summary(id).await else {
                continue;
            };
            if crate::memory::dreaming(&summary) {
                continue;
            }
            let name = summary.name;
            let reply = self.reply(id).await;
            lines.push(format!("- {id} {name}: {reply}"));
        }
        let text = c.memory().dream_input(c, &p.root).await;
        let text = format!(
            "{text}\n\n{}\n{}",
            crate::memory::dream_sessions_heading(),
            lines.join("\n")
        );
        let text = crate::memory::dream_text(&text);
        if due.is_some() && lines.is_empty() {
            return Ok(json!({"inactive": true}));
        }
        let at = now();
        let id = SessionId::new();
        // Record the scan's start, so activity while earlier projects dream reaches the next pass.
        if !c
            .memory()
            .dream_stamp(c, &p.root, p.through, p.since, &id)
            .await
            .map_err(other("claiming dreaming"))?
        {
            return Ok(json!({"already_run": true}));
        }
        let model = config.memory.model.map(crate::profiles::id);
        let session = if let Some(resident) = p.resident {
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
            self.schedule(&resident, "dream", &schedule, at, Some(id))
                .await?
                .1
        } else {
            let model = Some(model.unwrap_or(config.model));
            let id = c
                .create(
                    p.root,
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
        Ok(json!({"session": session}))
    }
}
