//! Project discovery and inputs for dreaming.
use super::{Controller, Error, Memory, Project, TEXT, blocking, files, other, root};
use crate::protocol::{Body, SessionId};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) struct Dream {
    pub(crate) root: PathBuf,
    pub(crate) resident: Option<SessionId>,
    pub(crate) since: u64,
    pub(crate) through: u64,
    pub(crate) run: Option<SessionId>,
    pub(crate) sessions: Vec<SessionId>,
}

impl Memory {
    pub(crate) async fn dream_project(&self, c: &Controller, cwd: &Path) -> Result<Dream, Error> {
        self.memory_fresh(c, cwd).await;
        self.files(c, cwd, |p| {
            let stamp = fs::read_to_string(p.memory.current[0].join(".dream")).unwrap_or_default();
            let mut lines = stamp.lines();
            Ok(Dream {
                root: p.root,
                resident: p.resident,
                through: 0,
                since: lines.next().and_then(|s| s.parse().ok()).unwrap_or(0),
                run: lines
                    .next()
                    .and_then(|s| SessionId::try_from(s.to_owned()).ok()),
                sessions: Vec::new(),
            })
        })
        .await
    }

    /// One discovery pass, bounded to seven days, with one root lookup per distinct folder.
    /// Run-now supplies its project and collects only its own sessions.
    pub(crate) async fn dream_sessions(
        &self,
        c: &Controller,
        only: Option<Dream>,
    ) -> Result<Vec<Dream>, Error> {
        let wanted = only.as_ref().map(|p| p.root.clone());
        let mut projects = only
            .into_iter()
            .map(|p| (p.root.clone(), Some(p)))
            .collect::<BTreeMap<_, _>>();
        let mut folders = BTreeMap::new();
        let settings = c.machine_settings().await;
        let through = crate::protocol::now();
        let cutoff = through.saturating_sub(7 * 86_400_000);
        let ids = crate::record::stored(c.home())
            .await
            .map_err(other("listing dreaming sources"))?;
        for id in ids {
            let path = c
                .home()
                .join("sessions")
                .join(id.as_str())
                .join("record.jsonl");
            let changed = tokio::fs::metadata(path)
                .await
                .and_then(|m| m.modified())
                .map(crate::protocol::millis)
                .unwrap_or(0);
            if changed <= cutoff {
                continue;
            }
            let Some(Body::Start {
                cwd, child, thread, ..
            }) = crate::record::start(c.home(), &id).await
            else {
                continue;
            };
            if thread.is_some_and(|t| t.job.as_deref() == Some("dream")) {
                continue;
            }
            let cwd = child
                .and_then(|c| c.worktree.map(|w| w.origin))
                .unwrap_or(cwd);
            if !folders.contains_key(&cwd) {
                let (settings, folder) = (settings.clone(), cwd.clone());
                let found = blocking(move || root(&settings, &folder))
                    .await
                    .ok()
                    .map(|(root, _)| root);
                folders.insert(cwd.clone(), found);
            }
            let Some(root) = folders[&cwd].as_ref() else {
                continue;
            };
            if wanted.as_ref().is_some_and(|w| w != root) {
                continue;
            }
            if !projects.contains_key(root) {
                projects.insert(root.clone(), self.dream_project(c, root).await.ok());
            }
            if let Some(project) = projects.get_mut(root).and_then(Option::as_mut)
                && changed > project.since
            {
                project.sessions.push(id);
            }
        }
        Ok(projects
            .into_values()
            .flatten()
            .map(|mut p| {
                p.through = through;
                p
            })
            .collect())
    }

    /// Stamp before starting: a crash loses this pass rather than repeating it.
    pub(crate) async fn dream_stamp(
        &self,
        c: &Controller,
        cwd: &Path,
        at: u64,
        expected: u64,
        session: &SessionId,
    ) -> Result<bool, Error> {
        let session = session.to_string();
        let result = self
            .memory_change(c, cwd, false, move |p| {
                if last(&p.memory.current[0]) != expected {
                    return Ok(serde_json::json!({"claimed": false}));
                }
                files::put(&p.memory.current[0], ".dream", &format!("{at}\n{session}"))?;
                Ok(serde_json::json!({"claimed": true}))
            })
            .await?;
        Ok(result["claimed"] == true)
    }

    pub(crate) async fn dream_input(&self, c: &Controller, cwd: &Path) -> String {
        let cites = self.citations(c, cwd).await;
        let (mut cites, more) = files::fit(cites.lines().map(str::to_owned), (usize::MAX, 8 << 10));
        if more > 0 {
            cites += &format!("\n{}", TEXT.dream_omitted.replace("{n}", &more.to_string()));
        }
        let scan = |p: Project| {
            let mut instructions = String::new();
            for folder in &p.memory.current {
                if let Ok(found) =
                    files::read(&folder.join("wiki/_instructions.md"), "wiki/_instructions")
                {
                    instructions += &format!("\n{}\n{}\n", folder.display(), found.file);
                }
            }
            Ok((instructions, files::known(&p.memory, (200, 16 << 10))?))
        };
        let (instructions, index) = self
            .files(c, cwd, scan)
            .await
            .unwrap_or_else(|e| (String::new(), e.to_string()));
        super::fill(
            &TEXT.dream_input,
            &[
                ("prompt", &include_str!("dream.md")),
                ("instructions", &instructions),
                ("index", &index),
                ("cites", &cites),
            ],
        )
    }
}

fn last(folder: &Path) -> u64 {
    fs::read_to_string(folder.join(".dream"))
        .ok()
        .and_then(|s| s.lines().next().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}
