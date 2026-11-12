//! Project discovery and inputs for dreaming.
use super::{
    Controller, Error, Memory, Project, TEXT, blocking, files, other, owned, root, share, tasks,
};
use crate::protocol::{BODY_BYTES, Body, SessionId};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) struct Dream {
    pub(crate) root: PathBuf,
    pub(crate) resident: Option<SessionId>,
    /// When a pass last claimed this server's night, and the time its sessions are read up to.
    pub(crate) claimed: u64,
    pub(crate) since: u64,
    pub(crate) through: u64,
    pub(crate) run: Option<SessionId>,
    /// The sessions to read, each with when its record last changed.
    pub(crate) sessions: Vec<(u64, SessionId)>,
    /// This server's id, as the stamp names it.
    pub(crate) server: String,
    /// What names this project on every server sharing its memory: that memory's first commit.
    shared: Option<String>,
    stamp: Stamp,
}

/// One server's part of a shared project's night, as it answers `GET /v1/dream/{id}`: its own id,
/// its person's email in the project's checkout, whether its own night runs the pass as the
/// project's resident, its clock, and the project's sessions its copy of the stamp calls unread.
#[derive(Serialize, Deserialize)]
pub(crate) struct Night {
    pub(crate) server: String,
    person: Option<String>,
    pub(crate) resident: bool,
    pub(crate) through: u64,
    pub(crate) sessions: Vec<(u64, SessionId)>,
}

/// A project's memory keeps one stamp, `.dream`: when the latest pass was claimed and its
/// session, then a line for each server with its id, when a pass last claimed its night, and the
/// time its sessions are read up to. A claim stops a second pass that night; only a pass that
/// ended moves what is read, so a server that dropped out is read the next night.
#[derive(Default)]
struct Stamp {
    at: u64,
    run: String,
    servers: BTreeMap<String, (u64, u64)>,
}

impl Stamp {
    fn read(folder: &Path) -> Stamp {
        let text = fs::read_to_string(folder.join(".dream")).unwrap_or_default();
        let mut lines = text.lines();
        let at = lines.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let run = lines.next().unwrap_or_default().to_owned();
        let mark = |line: &str| {
            let [server, claimed, read] = line.split(' ').collect::<Vec<_>>()[..] else {
                return None;
            };
            Some((
                server.to_owned(),
                (claimed.parse().ok()?, read.parse().ok()?),
            ))
        };
        let servers = lines.filter_map(mark).collect();
        Stamp { at, run, servers }
    }

    /// An unidentified server has never been read, including with an old two-line stamp.
    fn mark(&self, server: &str) -> (u64, u64) {
        self.servers.get(server).copied().unwrap_or_default()
    }

    fn write(&self, folder: &Path) -> Result<(), Error> {
        let line = |(server, (claimed, read)): (&String, &(u64, u64))| {
            format!("\n{server} {claimed} {read}")
        };
        let servers: String = self.servers.iter().map(line).collect();
        files::put(
            folder,
            ".dream",
            &format!("{}\n{}{servers}", self.at, self.run),
        )
    }
}

/// The lookback: a session, or a server's line in the stamp, older than seven days is left out.
const WEEK: u64 = 7 * 86_400_000;

impl Memory {
    pub(crate) async fn dream_project(&self, c: &Controller, cwd: &Path) -> Result<Dream, Error> {
        if !tokio::fs::metadata(cwd).await.is_ok_and(|m| m.is_dir()) {
            return Err(super::refuse(
                "The project folder does not exist; choose an existing folder.",
            ));
        }
        let p = self.files(c, cwd, Ok).await?;
        self.memory_fresh(&p).await;
        Ok(stamped(p).await)
    }

    /// The sessions whose records changed in the last seven days before `through`, each with its
    /// project's root and when it changed: one pass over the records, with one root lookup per
    /// distinct folder of a record from before they held their root.
    async fn recent(
        &self,
        c: &Controller,
        through: u64,
    ) -> Result<Vec<(PathBuf, u64, SessionId)>, Error> {
        let (mut folders, mut found) = (BTreeMap::new(), Vec::new());
        let settings = c.machine_settings().await;
        let cutoff = through.saturating_sub(WEEK);
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
                cwd,
                project_root,
                child,
                thread,
                ..
            }) = crate::record::start(c.home(), &id).await
            else {
                continue;
            };
            if thread.is_some_and(|t| t.job.as_deref() == Some("dream")) {
                continue;
            }
            let root = match project_root {
                Some(root) => root,
                // Records from before Start carried a root; delete once the seven-day lookback passes them.
                None => {
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
                    let Some(root) = folders[&cwd].clone() else {
                        continue;
                    };
                    root
                }
            };
            found.push((root, changed, id));
        }
        Ok(found)
    }

    /// Every active root and configured resident, with its unread sources. Neither the route
    /// nor scheduling may omit a clone, or a resident with no local records.
    async fn projects(&self, c: &Controller) -> Result<Vec<Dream>, Error> {
        let through = crate::protocol::now();
        let recent = self.recent(c, through).await?;
        let mut roots: Vec<_> = recent.iter().map(|(root, ..)| root.clone()).collect();
        roots.extend(
            c.machine_settings()
                .await
                .config
                .residents
                .values()
                .map(|r| r.cwd.clone()),
        );
        roots.sort();
        roots.dedup();
        let mut projects = Vec::new();
        for root in roots {
            let Ok(p) = self.files(c, &root, Ok).await else {
                continue;
            };
            let mut p = stamped(p).await;
            p.through = through;
            for (_, changed, id) in recent.iter().filter(|(r, ..)| *r == p.root) {
                if *changed > p.since && source(c, id).await {
                    p.sessions.push((*changed, id.clone()));
                }
            }
            if let Some(at) = projects
                .iter()
                .position(|other: &Dream| p.shared.is_some() && p.shared == other.shared)
            {
                if p.resident.is_some() {
                    std::mem::swap(&mut p, &mut projects[at]);
                }
                projects[at].sessions.extend(p.sessions);
            } else {
                projects.push(p);
            }
        }
        Ok(projects)
    }

    /// One discovery pass, bounded to seven days: each project with its sessions this server's
    /// line in its stamp calls unread, a pass's own left out.
    /// Run-now supplies its project and collects only its own sessions.
    pub(crate) async fn dream_sessions(
        &self,
        c: &Controller,
        only: Option<Dream>,
    ) -> Result<Vec<Dream>, Error> {
        let wanted = only.as_ref().map(|p| (p.root.clone(), p.shared.clone()));
        // These locations hold throwaway work, never automatic dreaming projects.
        const TEMP_FOLDERS: [&str; 2] = ["/tmp", "/var/tmp"];
        const RUNTIME_DIR: &str = "XDG_RUNTIME_DIR";
        const CACHE_DIR: &str = "XDG_CACHE_HOME";
        let excluded = TEMP_FOLDERS
            .into_iter()
            .map(PathBuf::from)
            .chain(Some(std::env::temp_dir()))
            .chain(std::env::var_os(RUNTIME_DIR).map(PathBuf::from))
            .chain(
                std::env::var_os(CACHE_DIR)
                    .map(PathBuf::from)
                    .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache"))),
            )
            .filter(|p| p.is_absolute())
            .collect::<Vec<_>>();
        let mut projects = self.projects(c).await?;
        if let Some(p) = only
            && !projects
                .iter()
                .any(|found| found.root == p.root || p.shared.is_some() && p.shared == found.shared)
        {
            projects.push(p);
        }
        projects.retain(|p| {
            wanted.as_ref().map_or_else(
                || !excluded.iter().any(|dir| p.root.starts_with(dir)),
                |(root, shared)| *root == p.root || shared.is_some() && *shared == p.shared,
            )
        });
        // Scheduling refreshes only eligible projects. The peer route must never fetch.
        let mut ready = Vec::new();
        for p in projects {
            let Ok(mut fresh) = self.dream_project(c, &p.root).await else {
                continue;
            };
            fresh.sessions = p
                .sessions
                .into_iter()
                .filter(|(at, _)| *at > fresh.since)
                .collect();
            fresh.through = p.through;
            ready.push(fresh);
        }
        Ok(ready)
    }

    /// All local checkouts sharing this memory's first commit, in one server response.
    pub(crate) async fn dream_night(&self, c: &Controller, id: &str) -> Result<Night, Error> {
        let p = self
            .projects(c)
            .await?
            .into_iter()
            .find(|p| p.shared.as_deref() == Some(id))
            .ok_or_else(|| super::refuse("No project here shares that memory."))?;
        let night = c.machine_settings().await.config.memory.night;
        Ok(Night {
            server: p.server,
            person: share::person(&p.root).await.map(|(_, email)| email),
            resident: p.resident.is_some() && !matches!(night, crate::config::DreamNight::Off(_)),
            through: p.through,
            sessions: p.sessions,
        })
    }

    /// The parts of shared project `p`'s night on the paired servers that answer and are its
    /// person's own, each with its name here: its sessions this server's stamp calls unread,
    /// named as `recall` reaches them. Another person's server is never read.
    pub(crate) async fn dream_peers(&self, c: &Controller, p: &Dream) -> Vec<(String, Night)> {
        let Some(id) = &p.shared else {
            return Vec::new();
        };
        let me = share::person(&p.root).await.map(|(_, email)| email);
        let (devices, mut nights) = (c.devices(), Vec::new());
        let path = format!("/v1/dream/{id}");
        for (device, _) in devices.paired().await {
            let night: Option<Night> = devices.fetch(&device, &path, &(), BODY_BYTES).await;
            let Some(mut night) = night.filter(|n| me.is_some() && n.person == me) else {
                continue;
            };
            let read = p.stamp.mark(&night.server).1;
            night.sessions.retain(|(changed, _)| *changed > read);
            for (_, session) in &mut night.sessions {
                *session = session.on(&device.name);
            }
            nights.push((device.name, night));
        }
        nights
    }

    /// Claims the night before the pass starts, for this server and each of `covered`, so a crash
    /// loses the night's pass rather than repeating it; refused when another pass claimed this
    /// server's night since `p` was read.
    pub(crate) async fn dream_stamp(
        &self,
        c: &Controller,
        p: &Dream,
        session: &SessionId,
        covered: Vec<String>,
    ) -> Result<bool, Error> {
        let (at, expected, session) = (p.through, p.claimed, session.to_string());
        let me = p.server.clone();
        let claim = move |stamp: &mut Stamp| {
            let claims = covered.iter().chain([&me]);
            let claims: Vec<_> = claims.map(|s| (s.clone(), (at, stamp.mark(s).1))).collect();
            let kept = |(claimed, read): &(u64, u64)| (*claimed).max(*read) + WEEK > at;
            stamp.servers.retain(|_, mark| kept(mark));
            stamp.servers.extend(claims);
            (stamp.at, stamp.run) = (at, session.clone());
        };
        self.stamp(c, p, Some(expected), claim).await
    }

    /// Only servers whose required digests were supplied to the completed pass advance.
    pub(crate) async fn dream_read(
        &self,
        c: &Controller,
        p: &Dream,
        read: Vec<(String, u64)>,
    ) -> Result<bool, Error> {
        let mark = move |stamp: &mut Stamp| {
            for (server, through) in &read {
                let (claimed, read) = stamp.mark(server);
                stamp
                    .servers
                    .insert(server.clone(), (claimed, read.max(*through)));
            }
        };
        self.stamp(c, p, None, mark).await
    }

    /// Changes `p`'s stamp on the remote, unless this server's claim in it is no longer `claimed`.
    async fn stamp(
        &self,
        c: &Controller,
        p: &Dream,
        claimed: Option<u64>,
        change: impl Fn(&mut Stamp) + Send + Sync + 'static,
    ) -> Result<bool, Error> {
        let (project, me) = (self.files(c, &p.root, Ok).await?, p.server.clone());
        let change = move |memory: &files::Folders| {
            let mut stamp = Stamp::read(&memory.current[0]);
            let held = claimed.is_none_or(|claimed| stamp.mark(&me).0 == claimed);
            if held {
                change(&mut stamp);
                stamp.write(&memory.current[0])?;
            }
            Ok(serde_json::json!(held))
        };
        let held = self.memory_change(&project, false, None, change).await?;
        Ok(held == true)
    }

    pub(crate) async fn dream_input(&self, c: &Controller, cwd: &Path, all: bool) -> String {
        let cites = self.citations(c, cwd, all).await;
        let (mut cites, more) = files::fit(cites.lines().map(str::to_owned), (usize::MAX, 8 << 10));
        if more > 0 {
            cites += &format!("\n{}", TEXT.dream_omitted.replace("{n}", &more.to_string()));
        }
        let scan = move |mut p: Project, me: Option<tasks::Person>| {
            if !all {
                p.memory.current.truncate(1);
            }
            let mut instructions = String::new();
            for folder in &p.memory.current {
                if let Ok(found) =
                    files::read(&folder.join("wiki/_instructions.md"), "wiki/_instructions")
                {
                    instructions += &format!("\n{}\n{}\n", folder.display(), found.file);
                }
            }
            let index = |m: &files::Folders| files::known(m, (200, 16 << 10));
            // The resident runs this pass, so its own layer is tidied in it too.
            let own = match p.resident.as_ref().map(|r| r.as_str()) {
                Some(name) => {
                    let (layer, head) = (p.memory.layer(Some(name)), &TEXT.dream_own);
                    let (index, rulings) = (index(&layer)?, files::rulings(&layer)?);
                    format!("\n\n{}\n{index}\n{rulings}", head.replace("{name}", name))
                }
                None => String::new(),
            };
            let (index, rulings) = (index(&p.memory)?, files::rulings(&p.memory)?);
            // What other people sharing the project ask this pass's person, with their readings.
            let all = tasks::scan(&p.tasks, false)?;
            let asks = me.map_or(Ok(Vec::new()), |me| tasks::asks(&p.tasks, &all, &me, true))?;
            let lead = (!asks.is_empty()).then(|| format!("\n\n{}", TEXT.dream_asks));
            let asks = files::fit(lead.into_iter().chain(asks), (64, 4 << 10)).0;
            Ok((instructions, index, rulings, own + &asks))
        };
        let read = async {
            let p = owned(self.files(c, cwd, Ok).await?).await;
            let me = super::me(&p).await;
            blocking(move || scan(p, me)).await
        };
        let (instructions, index, rulings, own) = read
            .await
            .unwrap_or_else(|e| (String::new(), e.to_string(), String::new(), String::new()));
        super::fill(
            &TEXT.dream_input,
            &[
                ("prompt", &include_str!("dream.md")),
                (
                    "scope",
                    if all {
                        &TEXT.dream_scope_all
                    } else {
                        &TEXT.dream_scope
                    },
                ),
                ("instructions", &instructions),
                ("index", &index),
                ("rulings", &rulings),
                ("own", &own),
                ("cites", &cites),
            ],
        )
    }
}

/// `p` as a pass finds it: its stamp, this server's line in it, and its shared memory's name.
async fn stamped(p: Project) -> Dream {
    let shared = match &p.memory_remote {
        Ok(Some(_)) => share::first(&p.memory.current[0]).await,
        _ => None,
    };
    let stamp = Stamp::read(&p.memory.current[0]);
    let (claimed, since) = stamp.mark(&p.server);
    Dream {
        root: p.root,
        resident: p.resident,
        server: p.server,
        claimed,
        since,
        through: crate::protocol::now(),
        run: SessionId::try_from(stamp.run.clone()).ok(),
        sessions: Vec::new(),
        shared,
        stamp,
    }
}

/// Whether session `id` is one a pass reads: any but a pass itself.
async fn source(c: &Controller, id: &SessionId) -> bool {
    c.summary(id).await.is_ok_and(|s| !super::dreaming(&s))
}
