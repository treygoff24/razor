//! Residents (docs/design/residents.md): one task fires every resident's heartbeat, schedules
//! and night through the controller; a resident's identity record; and the programs that carry
//! notifications and asks to the person.

mod calendar;
mod deliveries;
mod dream;
mod install;
mod list;
mod notify;
mod projects;
mod updates;

pub use deliveries::Deliveries;
pub(crate) use install::permissions;
pub use install::{
    DESKTOP_SOCKET, TEARDOWN_TIMEOUT, check_service, desktop, install, installed, server_lock,
    server_running, start, stop,
};
pub(crate) use list::{Listed, homes};
pub(crate) use notify::{ask, environment, failed, notify};
pub(crate) use updates::{release_action, release_ready};
pub use updates::{selected_executable, update_release};

use crate::config::{Config, Day, Deliver, Resident, Schedule, Time};
use crate::controller::{self, Controller};
use crate::protocol::views::{SectionKind, Views, ViewsStatus};
use crate::protocol::{Body, Effort, EntryId, Input, Millis, Origin, Run, SessionId, Thread, now};
use crate::record::Summary;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

/// The most an identity record, an ask command's answer, or a sky file may hold.
const FILE: u64 = 64 * 1024;
/// How far ahead `upcoming` looks, and how far back a row's runs go.
const DAY: Millis = 86_400_000;

#[derive(Deserialize)]
struct Text {
    ask: String,
    notify: String,
    option: String,
    identity: String,
    identity_heading: String,
    identity_thread: String,
    unreadable: String,
    too_large: String,
    unparsed: String,
    schedule_failed: String,
    schedule_stopped: String,
    no_resident: String,
    no_schedule: String,
    no_project: String,
    project_name: String,
    day: String,
    never_arrived: String,
    other: String,
    deliver_input: String,
    deliver_line: String,
    deliver_one: String,
    deliver_file: String,
    deliver_url: String,
    unit: String,
    plist: String,
    boot: String,
    login: String,
    no_manager: String,
    taken: String,
}

static TEXT: LazyLock<Text> = LazyLock::new(|| {
    // Invariant: the file is compiled into the binary, so it parses in every run of a build or
    // in none, and the first resident of any flow shows which.
    toml::from_str(include_str!("text.toml")).expect("src/residents/text.toml parses")
});

/// Every resident's heartbeat, schedules and night: one task in `razor serve`. Cheap to clone.
#[derive(Clone)]
pub struct Residents {
    inner: Arc<Inner>,
}

type Sort = (Option<SessionId>, Vec<SessionId>);

struct Inner {
    controller: Controller,
    state: Mutex<State>,
    sorting: watch::Sender<Sort>,
    dreaming: tokio::sync::Mutex<()>,
}

/// At most one entry per configured resident in `stopped` and `started`.
#[derive(Default)]
struct State {
    stopped: BTreeSet<SessionId>,
    /// When each running resident started in this process; at most one per configured home.
    started: BTreeMap<SessionId, Millis>,
    /// The fresh threads whose run's input is on its way: at most one per schedule being admitted.
    starting: BTreeSet<SessionId>,
    /// Closed fresh runs, by thread: only those a configured resident's rows still show.
    closed: BTreeMap<SessionId, Run>,
}

impl Residents {
    /// Start the task. On every wake, for each resident in the config that is not stopped:
    /// - On its first wake in this process it is started: `Controller::create` with the name as
    ///   the id, in its cwd, which makes the session the first time and opens it after that.
    /// - A schedule is admitted once when its latest due time is later than the latest `due` of
    ///   that name in the record, or, when the record has none, than the resident's start in
    ///   this process. The first occurrence of a schedule the record has never seen is lost if
    ///   Razor is down or crashes at its due time.
    /// - A heartbeat that came due since the last wake is admitted; `Busy` skips it.
    /// - A night that came due since the last wake asks the controller to rebuild.
    ///
    /// The task sleeps until the next due time, and at most 60 seconds, so a suspend or clock
    /// change takes effect within a minute; a settings change wakes it within about a second.
    pub fn open(controller: Controller) -> Residents {
        let (sorting, snapshots) = watch::channel(Sort::default());
        let residents = Residents {
            inner: Arc::new(Inner {
                controller,
                state: Mutex::default(),
                sorting,
                dreaming: tokio::sync::Mutex::new(()),
            }),
        };
        tokio::spawn(residents.clone().fire());
        tokio::spawn(residents.clone().sorts(snapshots));
        residents
    }

    /// Hand off a resolved snapshot without waiting for resident admission. One pending snapshot
    /// replaces the last while the single delivery task waits for queue space.
    pub(crate) fn sort(&self, definition: Option<&Views>, status: &ViewsStatus) {
        let auto = definition.and_then(|d| d.views.iter().find(|v| v.name == "Auto"));
        let wake = auto.and_then(|v| v.wake.clone());
        let roots = status
            .layouts
            .iter()
            .filter(|l| l.name == "Auto")
            .flat_map(|l| &l.sections)
            .filter(|s| s.kind == SectionKind::EverythingElse)
            .flat_map(|s| &s.sessions)
            .filter(|r| r.depth == 0)
            .map(|r| r.session.clone())
            .collect();
        self.inner.sorting.send_replace((wake, roots));
    }

    async fn sorts(self, mut snapshots: watch::Receiver<Sort>) {
        // Per-process deduplication, independent of row eviction and view edits.
        let mut sorted = BTreeSet::new();
        while snapshots.changed().await.is_ok() {
            let (wake, mut roots) = snapshots.borrow_and_update().clone();
            let Some(wake) = wake else {
                continue;
            };
            if !self.config().await.residents.contains_key(&wake) {
                continue;
            }
            roots.retain(|id| !sorted.contains(id));
            if !roots.is_empty() {
                let text = format!("New unmatched roots in Auto: {}", serde_json::json!(roots));
                let from = Origin::Program {
                    name: "smart-sort".into(),
                };
                tokio::select! {
                    biased;
                    _ = snapshots.changed() => snapshots.mark_changed(),
                    admitted = self.inner.controller.admit(&wake, Input::new(from, text)) => {
                        match admitted {
                            Ok(_) => sorted.extend(roots),
                            Err(e) => eprintln!("razor: Auto view could not wake {wake}: {e}"),
                        }
                    }
                }
            }
        }
    }

    /// Whether a resident's timers run in `session`: its home session, or a fresh run's thread.
    /// Neither sends an alert when a turn ends, nor is followed.
    pub(crate) async fn timed(&self, session: &Summary) -> bool {
        let job = session.thread.as_ref().is_some_and(|t| t.job.is_some());
        job || self.config().await.residents.contains_key(&session.session)
    }

    /// Run `schedule` now, as when it comes due, with `due` now: in the home session, waiting for
    /// a busy turn, or in a fresh thread. Its entry in the home session.
    pub(crate) async fn run(
        &self,
        name: &str,
        schedule: &str,
        cwd: Option<&Path>,
    ) -> Result<serde_json::Value, Error> {
        if schedule == "dream" {
            let cwd = match cwd {
                Some(cwd) => cwd.to_owned(),
                None => {
                    let name = SessionId::try_from(name.to_owned())
                        .map_err(other("reading the resident name"))?;
                    self.resident(&name).await?.cwd
                }
            };
            return self.dream_now(&cwd).await;
        }
        let name =
            SessionId::try_from(name.to_owned()).map_err(other("reading the resident name"))?;
        let name = &name;
        let resident = self.resident(name).await?;
        let Some(s) = resident.schedules.get(schedule) else {
            let sentence =
                (TEXT.no_schedule.replace("{name}", name.as_str())).replace("{schedule}", schedule);
            return Err(Error::Refused(sentence));
        };
        let (entry, _) = self.schedule(name, schedule, s, now(), None).await?;
        Ok(serde_json::json!({"entry": entry}))
    }

    /// A thread of resident `name`: its folder, or its resident project's, and the thread its
    /// Start records, the person's when `job` is none, else a fresh run's of that schedule.
    pub(crate) async fn thread(
        &self,
        name: &SessionId,
        job: Option<String>,
        resident_project: Option<String>,
    ) -> Result<(PathBuf, Option<Thread>), Error> {
        let cwd = match &resident_project {
            Some(id) => self.project(name, id).await?,
            None => self.resident(name).await?.cwd,
        };
        let thread = Thread {
            resident: name.clone(),
            job,
            resident_project,
        };
        Ok((cwd, Some(thread)))
    }

    /// Start a stopped resident, as at the server's start: its missed schedules run once.
    pub(crate) async fn start(&self, name: &SessionId) -> Result<(), Error> {
        self.resident(name).await?;
        let mut state = self.state();
        if state.stopped.remove(name) {
            state.started.remove(name);
        }
        drop(state);
        self.inner.controller.resident_wake().notify_one();
        Ok(())
    }

    /// Stop a resident: nothing of its schedule fires until `start` or the server's next start,
    /// and its current turn is stopped as by Escape. Its session and record stay. Input sent to
    /// it still runs, and that is how the person wakes it.
    pub(crate) async fn stop(&self, name: &SessionId) -> Result<(), Error> {
        self.resident(name).await?;
        self.state().stopped.insert(name.clone());
        let stopped = self.inner.controller.stop(name).await;
        stopped.map_err(other("stopping the resident's turn"))
    }

    async fn fire(self) {
        let mut last = now();
        let mut attempted = None::<tokio::time::Instant>;
        let mut release_attempted = None::<tokio::time::Instant>;
        loop {
            let config = self.config().await;
            if config.skill_updates
                && !config.provider_only
                && attempted.is_none_or(|at| at.elapsed() >= Duration::from_secs(86_400))
            {
                attempted = Some(tokio::time::Instant::now());
                let settings = self.inner.controller.machine_settings().await;
                let folders = crate::config::home_skills()
                    .into_iter()
                    .chain(settings.config.skills.iter().cloned())
                    .collect();
                tokio::spawn(crate::tools::update_skills(
                    folders,
                    settings.skill_overrides(),
                    settings.pack_skills(),
                ));
            }
            if config.release_updates
                && !config.provider_only
                && release_attempted.is_none_or(|at| at.elapsed() >= Duration::from_secs(86_400))
                && updates::installation().await.is_some()
            {
                release_attempted = Some(tokio::time::Instant::now());
                let config = config.clone();
                tokio::spawn(async move {
                    match updates::update_release(Some(&config)).await {
                        Ok(sentence) => eprintln!("razor: {sentence}"),
                        Err(error) => eprintln!("razor: release updates: {error}"),
                    }
                });
            }
            let now = now();
            if now / 60_000 > last / 60_000 {
                self.inner.controller.memory().tick(&self.inner.controller);
            }
            if let crate::config::DreamNight::At(night) = config.memory.night
                && let Some(due) = calendar::due(&[night], &[], now, true)
                && due > last
            {
                let residents = self.clone();
                tokio::spawn(async move { residents.dreams(due).await });
            }
            let mut sleep = 60_000;
            for (name, resident) in &config.residents {
                if self.state().stopped.contains(name) {
                    continue;
                }
                self.wake(name, resident, last, now).await;
                if let Some((_, at)) = next(resident, now) {
                    sleep = sleep.min(at.saturating_sub(now));
                }
            }
            let configured = |name: &SessionId| config.residents.contains_key(name);
            {
                let mut state = self.state();
                state.stopped.retain(|name| configured(name));
                state.started.retain(|name, _| configured(name));
            }
            last = last.max(now);
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(sleep)) => {}
                () = self.inner.controller.resident_wake().notified() => {}
            }
        }
    }

    /// One resident's wake at `now`: its start in this process, then whatever came due since
    /// `last`.
    async fn wake(&self, name: &SessionId, resident: &Resident, last: Millis, now: Millis) {
        let c = &self.inner.controller;
        let started = self.state().started.get(name).copied();
        let since = match started {
            Some(since) => since,
            None => {
                let (cwd, effort) = (resident.cwd.clone(), resident.effort);
                let model = resident.model.clone().map(crate::profiles::id);
                if let Err(e) = c
                    .create(cwd, model, (effort, None), Some(name.clone()), None, None)
                    .await
                {
                    return eprintln!("razor: resident {name} cannot start: {e}");
                }
                self.state().started.insert(name.clone(), now);
                now
            }
        };
        let came = |at: &[Time], days: &[Day]| {
            calendar::due(at, days, now, true).is_some_and(|due| due > last)
        };
        if let Some(h) = (resident.heartbeat.as_ref()).filter(|h| came(&h.at, &h.days)) {
            let heartbeat = input(Origin::Heartbeat, h.text.clone(), &h.model, h.effort);
            match c.admit(name, heartbeat).await {
                Ok(_) | Err(controller::Error::Busy(_)) => {}
                Err(e) => eprintln!("razor: resident {name}'s heartbeat: {e}"),
            }
        }
        let recorded = match c.summary(name).await {
            Ok(summary) => summary.scheduled,
            Err(e) => return eprintln!("razor: resident {name}'s schedules: {e}"),
        };
        for (schedule, s) in resident.schedules.iter().take(64) {
            let Some(due) = calendar::due(&s.at, &s.days, now, true) else {
                continue;
            };
            if due <= recorded.get(schedule).copied().unwrap_or(since) {
                continue;
            }
            if let Err(e) = self.schedule(name, schedule, s, due, None).await {
                eprintln!("razor: resident {name}'s schedule {schedule}: {e}");
            }
        }
        if came(&[resident.night], &[]) {
            let from = Origin::Program {
                name: "razor".into(),
            };
            let input = c
                .admit(name, Input::new(from, TEXT.day.clone()))
                .await
                .inspect_err(|e| eprintln!("razor: resident {name}'s day: {e}"))
                .ok();
            if let Err(e) = c.night(name, input).await {
                eprintln!("razor: resident {name}'s night: {e}");
            }
        }
    }

    /// Admit a schedule's input as when due: in the home session, or in a fresh thread named
    /// for it after the home session records the run, so a crash between them loses the run
    /// rather than repeating it. The schedule's entry in the home session.
    async fn schedule(
        &self,
        name: &SessionId,
        schedule: &str,
        s: &Schedule,
        due: Millis,
        id: Option<SessionId>,
    ) -> Result<(EntryId, SessionId), Error> {
        let c = &self.inner.controller;
        let text = match s.deliver {
            Deliver::None => s.text.clone(),
            Deliver::Ask => format!("{}\n\n{}", s.text, TEXT.ask),
            Deliver::Notify => format!("{}\n\n{}", s.text, TEXT.notify),
        };
        let from = Origin::Schedule {
            name: schedule.into(),
            due,
        };
        let input = input(from, text, &s.model, s.effort);
        if !s.fresh {
            return c
                .admit(name, input)
                .await
                .map(|entry| (entry, name.clone()))
                .map_err(other("admitting a schedule"));
        }
        let (cwd, thread) = self.thread(name, Some(schedule.into()), None).await?;
        let created = c.create(cwd, input.model.clone(), (s.effort, None), id, None, thread);
        let session = created
            .await
            .map_err(other("starting a fresh run's thread"))?;
        let name_it = Body::Name {
            name: format!("{schedule} · {}", calendar::label(due)),
        };
        c.append(&session, name_it)
            .await
            .map_err(other("naming a fresh run's thread"))?;
        self.state().starting.insert(session.clone());
        let fresh = Body::Fresh {
            schedule: schedule.into(),
            due,
            session: session.clone(),
        };
        let started = match c.append(name, fresh).await {
            Ok(entry) => c.admit(&session, input).await.map(|_| entry),
            Err(e) => Err(e),
        };
        self.state().starting.remove(&session);
        started
            .map(|entry| (entry, session))
            .map_err(other("starting a fresh run"))
    }

    /// The resident's config; `NoResident` when there is none.
    pub(crate) async fn resident(&self, name: &SessionId) -> Result<Resident, Error> {
        let resident = self.config().await.residents.remove(name);
        resident.ok_or_else(|| Error::NoResident(name.clone()))
    }

    async fn config(&self) -> Config {
        self.inner.controller.config().await
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while the lock is held is already a bug; the sets are still whole.
        self.inner.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The resident's heartbeat, schedules and night, each named as runs and `next` name it.
fn timers(resident: &Resident) -> impl Iterator<Item = (&str, &[Time], &[Day])> {
    let heartbeat = (resident.heartbeat.iter()).map(|h| ("heartbeat", &h.at[..], &h.days[..]));
    let schedules = (resident.schedules.iter().take(64))
        .map(|(name, s)| (name.as_str(), &s.at[..], &s.days[..]));
    let night = std::iter::once(("night", std::slice::from_ref(&resident.night), &[][..]));
    heartbeat.chain(schedules).chain(night)
}

/// What of `resident` comes due first after `now` (`heartbeat`, a schedule's name, or `night`),
/// and when: through the same function that fires it.
fn next(resident: &Resident, now: Millis) -> Option<(&str, Millis)> {
    (timers(resident))
        .filter_map(|(what, at, days)| Some((what, calendar::due(at, days, now, false)?)))
        .min_by_key(|(_, at)| *at)
}

fn input(from: Origin, text: String, model: &Option<String>, effort: Option<Effort>) -> Input {
    Input {
        from,
        text,
        model: model.clone().map(crate::profiles::id),
        effort,
        ..Default::default()
    }
}

/// A resident's identity record as its first input carries it: the heading naming
/// `identity.toml` in its session's directory (made from a template at its first start), then
/// the record, at most 64 KiB. A record that cannot be read, or does not parse, becomes one
/// sentence saying so, so the resident can fix it. None for a session that is not a resident.
/// A thread reads its resident's record, and its heading says the state note is the home's.
pub(crate) async fn identity(
    config: &Config,
    session: &SessionId,
    dir: &Path,
    thread: bool,
) -> Option<String> {
    config.residents.get(session)?;
    let path = dir.join("identity.toml");
    let mut new = tokio::fs::OpenOptions::new();
    if let Ok(mut file) = new.write(true).create_new(true).open(&path).await {
        let template = TEXT.identity.replace("{name}", session.as_str());
        // A failed write shows in the read below.
        let _ = file.write_all(template.as_bytes()).await;
    }
    let mut heading = TEXT
        .identity_heading
        .replace("{path}", &path.display().to_string());
    if thread {
        heading = format!("{heading} {}", TEXT.identity_thread);
    }
    Some(format!("{heading}\n\n{}", read(&path).await))
}

/// The identity record's text, or the sentence saying why it cannot be used.
async fn read(path: &Path) -> String {
    let t = &*TEXT;
    let shown = path.display().to_string();
    let mut text = String::new();
    let read = async {
        let file = tokio::fs::File::open(path).await?;
        file.take(FILE + 1).read_to_string(&mut text).await
    };
    match read.await {
        Err(e) => (t.unreadable.replace("{path}", &shown)).replace("{why}", &e.to_string()),
        Ok(n) if n as u64 > FILE => t.too_large.replace("{path}", &shown),
        Ok(_) => match toml::from_str::<toml::Table>(&text) {
            Err(e) => (t.unparsed.replace("{path}", &shown)).replace("{why}", e.message()),
            Ok(_) => text,
        },
    }
}

/// Why a residents operation failed.
#[derive(Debug)]
pub enum Error {
    /// No resident by this name in the config.
    NoResident(SessionId),
    /// The sentence saying the resident has no such project.
    NoProject(String),
    /// One sentence for the model or the person: the `deliver` action's refusal, no notify
    /// command, a config value that does not parse.
    Refused(String),
    /// Any other failure, with its cause and what was being done.
    Other {
        source: Box<dyn std::error::Error + Send + Sync>,
        context: &'static str,
    },
}

fn other<E>(context: &'static str) -> impl FnOnce(E) -> Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    move |e| Error::Other {
        source: e.into(),
        context,
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoResident(name) => {
                f.write_str(&TEXT.no_resident.replace("{name}", name.as_str()))
            }
            Error::Refused(sentence) | Error::NoProject(sentence) => f.write_str(sentence),
            Error::Other { source, context } => f.write_str(
                &(TEXT.other)
                    .replace("{context}", context)
                    .replace("{source}", &source.to_string()),
            ),
        }
    }
}

impl std::error::Error for Error {}
