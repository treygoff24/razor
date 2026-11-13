//! Memory and tasks (docs/design/memory.md): the startup page and the
//! `memory` and `task` actions, on Razor's own files, and the rules for their writes. Tasks
//! share memory's files, page and project; a project's tasks may be shared through git.

mod citations;
mod claims;
mod dream;
pub(crate) use dream::{Dream, Night};
mod files;
mod share;
mod shared;
mod tasks;
mod write;
use shared::{offered, owned};

use crate::controller::Controller;
use crate::profiles::machine;
use crate::protocol::{Body, EntryId, Input, Origin, SessionId};
use crate::record::{Record, Summary};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};
use tasks::{Edit, Listing};

/// A person's task list query; the path's session is its viewer.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct TaskQuery {
    pub(crate) filter: Option<String>,
    pub(crate) q: String,
    pub(crate) limit: Option<usize>,
    pub(crate) mine: bool,
}

/// The most bytes of a state note, written or shown.
const STATE: usize = 1500;

#[derive(Deserialize)]
struct Text {
    heads: [String; 3],
    own: [String; 3],
    cites: String,
    page_only: String,
    page_name: String,
    dream_ruling: String,
    ruling_line: String,
    ruling_expires: String,
    ruling_sources: String,
    replacements: String,
    project_all: String,
    project_path: String,
    file_long: String,
    scope: String,
    pages_full: String,
    instructions: String,
    dream_input: String,
    dream_scope: String,
    dream_scope_all: String,
    dream_sessions: String,
    dream_omitted: String,
    no_state: String,
    none: String,
    more: String,
    input: String,
    child: String,
    thread: String,
    too_long: String,
    state_long: String,
    state_all: String,
    no_text: String,
    kind: String,
    expires: String,
    description: String,
    no_person: String,
    not_person: String,
    no_memory: String,
    gone: String,
    other: String,
    tasks: String,
    shared: String,
    held: String,
    owed: String,
    due_on: String,
    task_input: String,
    task_child: String,
    filter: String,
    no_task: String,
    closed: String,
    bounds: String,
    due: String,
    claimed: String,
    ready: String,
    waits: String,
    overdue: String,
    offline: String,
    stale_read: String,
    rebase: String,
    rollback: String,
    not_written: String,
    push_unknown: String,
    paths_incomplete: String,
    git_lock: String,
    git_lock_other: String,
    nested_memory: String,
    changed: String,
    settings: String,
    identity: String,
    owner: String,
    owned: [String; 2],
    differ: String,
    settles: String,
    set_aside: String,
    restore: String,
    join: String,
    unjoined: String,
    asks: String,
    asks_more: String,
    asked: String,
    asks_whom: String,
    glossary: [String; 2],
    dream_asks: String,
    own_scope: String,
    dream_own: String,
    secret: String,
    secret_keys: Vec<String>,
    secret_block: String,
}

static TEXT: LazyLock<Text> = LazyLock::new(|| {
    // Invariant: the file is compiled into the binary, so it parses in every run of a build or
    // in none, and the first memory call of any flow shows which.
    toml::from_str(include_str!("text.toml")).expect("src/memory/text.toml parses")
});

/// Memory and tasks for one server, under `<home>/memory` and `<home>/tasks`. Between calls it
/// holds one lock per task store and one for memory, plus a lock preventing overlapping ticks.
/// File reads take none, since every file is replaced by one rename.
pub(crate) struct Memory {
    home: PathBuf,
    stores: Mutex<BTreeMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    tick: tokio::sync::Mutex<()>,
    /// Whether releases may be owed: set at start and when a release fails, so the next tick
    /// looks for claims that a closed or settled session still holds.
    owed: AtomicBool,
}

/// The session making a `memory` or `task` call: what the call needs from it. The controller
/// fills it.
pub(crate) struct Caller<'a> {
    pub(crate) session: &'a SessionId,
    /// The call's own entry. A memory it writes cites `razor:<session>:l-<at>`.
    pub(crate) at: EntryId,
    pub(crate) cwd: &'a Path,
    /// A child only claims and comments on existing tasks; memory writes are refused.
    pub(crate) child: bool,
    /// The owned entry at `ref`, or at the latest person input when `ref` is absent.
    /// The controller fetches it with [`cited`] before queueing; memory validates its origin.
    pub(crate) cited: Option<Cited>,
}

/// An entry a ruling may cite and, when it is the person's input or answer, their words.
pub(crate) struct Cited {
    id: EntryId,
    said: Option<String>,
    ask: Option<EntryId>,
}

/// The entry `id` of `record` as a ruling may cite it.
pub(crate) async fn cited(record: &Record, id: EntryId) -> Option<Cited> {
    let (said, ask) = match record.entry(id).await.ok()?.body {
        Body::Input(input) => match input.from {
            Origin::Person => (Some(input.text), None),
            Origin::Answer { ask } => (Some(input.text), Some(ask)),
            _ => (None, None),
        },
        _ => (None, None),
    };
    Some(Cited { id, said, ask })
}

/// One `memory` call's input.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Call {
    Search { query: String, limit: Option<usize> },
    Get { id: String },
    Write(Box<Write>),
}

/// One `task` call's input.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum TaskCall {
    List(Listing),
    Get { id: u32 },
    Write(Edit),
}

/// A write as the model asks it; the shared rules then compose its text and fill its source.
#[derive(Clone, Deserialize)]
struct Write {
    kind: Option<Kind>,
    #[serde(default)]
    text: String,
    /// One line saying what a fact or note is, for the startup page.
    description: Option<String>,
    #[serde(default)]
    all: bool,
    /// A date, `2026-12-31`, through which it applies.
    expires: Option<String>,
    replaces: Option<Replaces>,
    #[serde(default)]
    cites: Vec<String>,
    /// Other people's rulings a new ruling of the person's agreement sets aside.
    #[serde(default)]
    settles: Vec<String>,
    name: Option<String>,
    checked: Option<String>,
    #[serde(rename = "ref")]
    cite: Option<EntryId>,
    /// The entry the memory comes from: never the model's to set.
    #[serde(skip)]
    source: String,
    #[serde(skip)]
    session: String,
    #[serde(skip)]
    dreaming: bool,
    #[serde(skip)]
    quotes: Vec<(String, String)>,
    #[serde(skip)]
    question: Option<String>,
}

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(untagged)]
enum Replaces {
    One(String),
    Many(Vec<String>),
}

impl Replaces {
    fn ids(&self) -> &[String] {
        match self {
            Self::One(id) => std::slice::from_ref(id),
            Self::Many(ids) => ids,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Ruling,
    Fact,
    Note,
    State,
    Page,
}

/// Where a session keeps memory and tasks, found at each call from the person's settings: the
/// folder of the resident whose folder holds the session's (the closest when several do), else
/// its project's main checkout.
#[derive(Clone)]
struct Project {
    root: PathBuf,
    memory: files::Folders,
    tasks: PathBuf,
    /// The resident whose folder it is, whose own sessions keep a layer of their own.
    resident: Option<SessionId>,
    /// The remote the project's private settings name, which its tasks are shared through, or
    /// why those settings do not read.
    remote: Result<Option<String>, String>,
    memory_remote: Result<Option<String>, String>,
    /// Whether to offer joining what its origin shares: it names no remote and was not declined.
    join: bool,
    /// This server's id: a claim names it, and only this server releases the claim.
    server: String,
}

/// What a memory call answers: the store's own JSON, or why it failed.
type Answer = Result<Value, Error>;
/// A note for an open session, its id and its text, from the program `razor`.
type Note = (String, String);

impl Memory {
    /// Memory for the server whose home is `home`. It reads and writes nothing until a call.
    pub(crate) fn new(home: &Path) -> Memory {
        Memory {
            home: home.into(),
            stores: Default::default(),
            tick: Default::default(),
            owed: true.into(),
        }
    }

    /// One mutex per store touched during this server run; the map is held only for lookup.
    fn store(&self, dir: &Path) -> Arc<tokio::sync::Mutex<()>> {
        self.stores
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(dir.into())
            .or_default()
            .clone()
    }

    /// The home session a resident's thread `s` sends its closing state to; none for any other
    /// session, which writes its own state note.
    pub(crate) async fn state_owner(
        &self,
        c: &Controller,
        s: &Summary,
    ) -> Result<Option<String>, Error> {
        let p = self.files(c, &s.cwd, Ok).await?;
        Ok(own(&p, s).filter(|_| s.thread.is_some()))
    }

    /// The startup page for session `s`: the project's state note (at most 1,500 bytes);
    /// the standing rulings, newest first, at most 20 lines in 4,000 bytes, each a rule, its owner
    /// on a shared project, and its id; a line for each fact and note, newest first, at most 24 in
    /// 2,400 bytes, the project's glossary taking its lines from them; for a resident's own
    /// session, the same of its own layer; then the tasks it holds and those due within a week,
    /// and how many are ready, or, for a shared project with no open task here, that `task list`
    /// reads them; and at most five questions for the person, other people's among them. A
    /// line that does not fit is passed over, not cut. It never fails: a page that cannot be read
    /// is one sentence saying why. It never fetches.
    pub(crate) async fn page(&self, c: &Controller, s: &Summary) -> String {
        let p = match self.files(c, &s.cwd, Ok).await {
            Ok(p) => owned(p).await,
            Err(e) => return e.to_string(),
        };
        let offered = offered(&p).await.then(|| TEXT.join.clone());
        let mut unjoined = Vec::new();
        for (dir, remote) in [
            (&p.memory.current[0], &p.memory_remote),
            (&p.tasks, &p.remote),
        ] {
            if s.child.is_none()
                && matches!(remote, Ok(Some(_)))
                && share::unjoined(dir).await.unwrap_or(false)
            {
                unjoined.push(fill(&TEXT.unjoined, &[("dir", &dir.display())]));
            }
        }
        let (own, session) = (own(&p, s), s.session.to_string());
        let me = me(&p).await;
        let read = blocking(move || {
            let (t, shared) = (&*TEXT, (matches!(p.remote, Ok(Some(_))), me.as_ref()));
            let (tasks, questions) =
                tasks::page(&p.tasks, &session, &p.server, shared, own.as_deref())?;
            let (mut page, mut asks) = files::page(&p.memory, &t.heads)?;
            asks.extend(questions);
            // No one else's agreement can name a layer's ruling, so it asks nothing.
            if let Some(name) = own {
                let heads = t.own.each_ref().map(|head| head.replace("{name}", &name));
                let layer = files::page(&p.memory.layer(Some(&name)), &heads)?.0;
                page += &format!("\n\n{layer}");
            }
            Ok((page, asks, tasks))
        });
        let (page, asks, tasks) = match read.await {
            Ok(read) => read,
            Err(e) => return e.to_string(),
        };
        // A helper does not talk with the person, so it is asked nothing.
        let asks = unjoined.into_iter().chain(offered).chain(asks);
        let (mut asks, more) = files::fit(asks.filter(|_| s.child.is_none()), (5, 2400));
        if more > 0 {
            asks += &format!("\n{}", TEXT.asks_more.replace("{n}", &more.to_string()));
        }
        let asks = (!asks.is_empty()).then(|| format!("{}\n{asks}", TEXT.asks));
        let page = format!("{page}\n\n{tasks}\n\n{}", asks.unwrap_or_default());
        page.trim_end().into()
    }

    /// The `memory` and `task` actions (docs/design/memory.md). `c` names a task
    /// writer, sets a claimant's headline, and admits the notes a change causes.
    pub(crate) async fn run(
        &self,
        c: &Controller,
        tool: &str,
        at: Caller<'_>,
        input: Value,
    ) -> Answer {
        #[derive(Deserialize)]
        struct Target {
            project: Option<PathBuf>,
            #[serde(default)]
            all: bool,
        }
        let target: Target = Target::deserialize(&input)
            .map_err(|e| refuse(&TEXT.input.replace("{why}", &e.to_string())))?;
        if target.project.is_some() && target.all {
            return Err(refuse(&TEXT.project_all));
        }
        let cwd = target.project.map(|path| at.cwd.join(path));
        if let Some(path) = &cwd
            && !tokio::fs::metadata(path).await.is_ok_and(|m| m.is_dir())
        {
            return Err(refuse(&TEXT.project_path));
        }
        let at = Caller {
            cwd: cwd.as_deref().unwrap_or(at.cwd),
            ..at
        };
        if tool == "task" {
            return self.task(c, &at, input).await;
        }
        let resident = input.get("resident").map_or(Ok(false), bool::deserialize);
        let resident =
            resident.map_err(|e| refuse(&TEXT.input.replace("{why}", &e.to_string())))?;
        let s = c.summary(at.session).await;
        let s = s.map_err(other("reading the memory caller"))?;
        let p = self.files(c, at.cwd, Ok).await?;
        let layer = own(&p, &s).filter(|_| resident);
        if resident && (target.all || layer.is_none()) {
            return Err(refuse(&TEXT.own_scope));
        }
        let call = serde_json::from_value(input)
            .map_err(|e| refuse(&TEXT.input.replace("{why}", &e.to_string())))?;
        let memory = p.memory.layer(layer.as_deref());
        match call {
            Call::Write(write) => self.write(&at, p, &s, resident, *write).await,
            Call::Search { query, limit } => {
                self.memory_fresh(&p).await;
                let limit = limit.unwrap_or(8).clamp(1, 20);
                blocking(move || files::search(&memory, &query, limit)).await
            }
            Call::Get { id } => {
                self.memory_fresh(&p).await;
                blocking(move || files::get(&memory, &id)).await
            }
        }
    }

    /// For the person's phone: `{"hits": [{id, kind, summary, updated_at}], "omitted_count"}`,
    /// the newest active memories of `cwd`'s project and of every project.
    pub(crate) async fn list(&self, c: &Controller, cwd: &Path) -> Answer {
        let list = |p: Project| files::search(&p.memory, "", files::KEEP);
        self.files(c, cwd, list).await
    }

    /// At the person's word, deletes memory `id` of `cwd`'s project or of every project, words
    /// and all, so a deleted memory is gone, not only hidden.
    pub(crate) async fn forget(&self, c: &Controller, cwd: &Path, id: &str) -> Answer {
        let (id, p) = (id.to_owned(), self.files(c, cwd, Ok).await?);
        let (memory, lookup) = (p.memory.clone(), id.clone());
        let all = blocking(move || files::global(&memory, &lookup)).await?;
        let forget = move |m: &files::Folders| files::retire(m, std::slice::from_ref(&id), true);
        self.memory_change(&p, all, None, forget).await
    }

    /// The project's tasks as seen by `session`, or one structured task when `id` is given.
    pub(crate) async fn tasks(
        &self,
        c: &Controller,
        cwd: &Path,
        session: &SessionId,
        q: TaskQuery,
        id: Option<u32>,
    ) -> Answer {
        let session = session.to_string();
        self.read(c, cwd, move |p| {
            let l = Listing {
                filter: q.filter,
                query: q.q,
                mine: q.mine,
                limit: Some(q.limit.unwrap_or(50).clamp(1, 200)),
                viewer: (session, p.server.clone()),
                ..Listing::default()
            };
            let mut answer = match id {
                Some(id) => tasks::detail(&p.tasks, id, &l.viewer),
                None => tasks::list(&p.tasks, &l),
            }?;
            answer["project"] = serde_json::json!(p.root);
            Ok(answer)
        })
        .await
    }

    /// A `task` call: a child may only claim or comment on an existing task; a list or a get
    /// reads the project's tasks.
    async fn task(&self, c: &Controller, at: &Caller<'_>, input: Value) -> Answer {
        if at.child && input["action"] == "write" {
            let allowed = |k: &String| {
                matches!(
                    k.as_str(),
                    "action" | "id" | "claim" | "comment" | "project"
                )
            };
            if input["id"].as_u64().is_none()
                || input.as_object().is_some_and(|o| !o.keys().all(allowed))
            {
                return Err(refuse(&TEXT.task_child));
            }
        }
        let call = serde_json::from_value(input);
        let call = call.map_err(|e| refuse(&fill(&TEXT.task_input, &[("why", &e)])))?;
        match call {
            TaskCall::Write(mut edit) => {
                edit.own = at.child;
                let writer = c.summary(at.session).await;
                let writer = writer.map_err(other("reading the task writer"))?;
                self.edit(c, at.cwd, (at.session, &writer.name), edit).await
            }
            TaskCall::List(mut l) => {
                let session = at.session.to_string();
                self.read(c, at.cwd, move |p| {
                    l.viewer = (session, p.server.clone());
                    tasks::list(&p.tasks, &l)
                })
                .await
            }
            TaskCall::Get { id } => {
                self.read(c, at.cwd, move |p| tasks::get(&p.tasks, id))
                    .await
            }
        }
    }

    /// `op` on `cwd`'s tasks, first brought up to date when they are shared and the last fetch
    /// is over 30 seconds old; a failed refresh returns local data with a warning.
    async fn read<F>(&self, c: &Controller, cwd: &Path, op: F) -> Answer
    where
        F: FnOnce(&Project) -> Answer + Send + 'static,
    {
        let p = self.files(c, cwd, Ok).await?;
        let c = c.clone();
        tokio::spawn(async move {
            let memory = c.memory();
            let mut refresh = None;
            if matches!(p.remote, Ok(Some(_))) && share::stale(&p.tasks).await {
                let held = memory.store(&p.tasks).lock_owned().await;
                let (answer, notes) = memory.locked(&c, &p, None).await;
                refresh = answer.err();
                drop(held);
                send(&c, notes).await;
            }
            let mut answer = blocking(move || op(&p)).await;
            if let Some(why) = refresh {
                let warning = fill(&TEXT.stale_read, &[("why", &why)]);
                match &mut answer {
                    Ok(value) => value["warning"] = warning.into(),
                    Err(why) => *why = refuse(&format!("{why}; {warning}")),
                }
            }
            answer
        })
        .await
        .map_err(other("waiting for the task read"))?
    }

    /// One change, `edit`, to `cwd`'s tasks by session `by.0`, named `by.1`. On a shared project
    /// it is on the remote before it answers. The notes it causes go out once the lock is
    /// released, and a claim becomes the claimant's headline.
    async fn edit(&self, c: &Controller, cwd: &Path, by: By<'_>, edit: Edit) -> Answer {
        let (c, cwd, session, name) = (c.clone(), cwd.to_owned(), by.0.clone(), by.1.to_owned());
        tokio::spawn(async move { c.memory().editing(&c, &cwd, (&session, &name), edit).await })
            .await
            .map_err(other("waiting for the task write"))?
    }

    /// Owns the complete write and its lock; stopping the caller only stops waiting for it.
    async fn editing(&self, c: &Controller, cwd: &Path, by: By<'_>, edit: Edit) -> Answer {
        let p = self.files(c, cwd, Ok).await?;
        let held = self.store(&p.tasks).lock_owned().await;
        // A question is for someone who has written to the shared list, as the remote now has
        // it, so none waits on a name no startup page matches.
        let mut owed = Vec::new();
        if let Some(name) = edit.asks.as_deref().filter(|name| !name.is_empty()) {
            let mut people = Vec::new();
            if matches!(p.remote, Ok(Some(_))) {
                owed = self.locked(c, &p, None).await.1;
                people = share::people(&p.tasks).await;
            }
            if !people.iter().any(|(n, email)| name == n || name == email) {
                drop(held);
                send(c, owed).await;
                let names: Vec<_> = people.iter().map(|(name, _)| name.as_str()).collect();
                return Err(refuse(
                    &TEXT.asks_whom.replace("{names}", &names.join(", ")),
                ));
            }
        }
        let (session, name) = by;
        let who = who(session, &p.server, name);
        let (answer, notes) = self.locked(c, &p, Some((who, edit))).await;
        drop(held);
        owed.extend(notes);
        send(c, owed).await;
        let claimed = answer.as_ref().ok().filter(|a| a["claimed"] == true);
        if let Some(title) = claimed.and_then(|a| a["title"].as_str()) {
            let headline = Body::Headline { text: title.into() };
            let _ = c.append(session, headline).await;
        }
        answer
    }

    /// Under the lock: `change`, by its writer in a claim's form, applied to `p`'s tasks; when
    /// they are shared, against the remote's as fetched within 30 seconds and pushed before it
    /// counts, and run again from a fetch, at most twice more, when another machine pushed first
    /// or a refusal came from a view not just fetched.
    /// Its answer, and the notes owed for the tasks closed meanwhile, here or on the remote.
    async fn locked(
        &self,
        c: &Controller,
        p: &Project,
        change: Option<(String, Edit)>,
    ) -> (Answer, Vec<Note>) {
        let (dir, server) = (p.tasks.clone(), p.server.clone());
        let at = dir.clone();
        let before = blocking(move || tasks::scan(&at, false)).await;
        let by = change.as_ref().map(|(who, _)| tasks::shown(who).to_owned());
        let project = p.clone();
        let op = change
            .clone()
            .map(|(who, edit)| move || tasks::apply(&project.tasks, &who, &edit));
        let (answer, synced) =
            share::change(&p.root, &dir, &p.remote, "refs/razor/tasks", op, |done| {
                format!(
                    "task {}, by {}",
                    done["id"],
                    by.as_deref().unwrap_or_default()
                )
            })
            .await;
        if synced || change.is_some() && answer.is_ok() {
            c.tasks_changed(p.root.clone());
        }
        let closes = blocking(move || tasks::closes(&dir, &server, &before.unwrap_or_default()));
        (answer, closes.await.unwrap_or_default())
    }

    /// `op` on `cwd`'s project, on a thread of its own, off the loop.
    async fn files<T: Send + 'static>(
        &self,
        c: &Controller,
        cwd: &Path,
        op: impl FnOnce(Project) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let (home, cwd) = (self.home.clone(), cwd.to_owned());
        let settings = c.machine_settings().await;
        blocking(move || op(project(&home, &settings, &cwd)?)).await
    }
}

/// The project a session in `cwd` keeps memory and tasks for, from `settings`, the machine's as
/// last read good, and the project's own as they now read.
fn project(home: &Path, settings: &crate::config::Settings, cwd: &Path) -> Result<Project, Error> {
    let (root, resident) = root(settings, cwd)?;
    // A project's own settings that do not read add errors past the machine's.
    let config = settings.project(&root).map_err(|e| e.to_string());
    let config = config.and_then(|s| match s.errors.len() > settings.errors.len() {
        true => Err(s.errors.join(" ")),
        false => Ok(s.config),
    });
    let remote = config
        .as_ref()
        .map(|c| c.tasks.remote.clone())
        .map_err(Clone::clone);
    let alone = |c: &crate::config::Config| c.memory.remote.is_none() && c.tasks.remote.is_none();
    let join = config.as_ref().is_ok_and(|c| c.memory.join && alone(c));
    let memory_remote = config.map(|c| c.memory.remote);
    let server = std::fs::read_to_string(home.join("server.id")).unwrap_or_default();
    Ok(Project {
        memory: files::folders(&home.join("memory"), &root),
        tasks: home.join("tasks").join(tasks::folder(&root)),
        server: server.trim().to_owned(),
        root,
        resident,
        remote,
        memory_remote,
        join,
    })
}

/// The person whose home this is, by name and email, where `p`'s tasks are shared: the one a
/// question in the shared list may be for.
async fn me(p: &Project) -> Option<tasks::Person> {
    match p.remote {
        Ok(Some(_)) => share::person(&p.root).await,
        _ => None,
    }
}

/// The resident whose own layer session `s` may use in project `p`: by its record, `s` is that
/// resident's home session or one of its threads, and `p` is the resident's project.
fn own(p: &Project, s: &Summary) -> Option<String> {
    let me = s.thread.as_ref().map_or(&s.session, |t| &t.resident);
    let own = p.resident.as_ref().filter(|r| *r == me);
    own.map(|r| r.as_str().into())
}

/// The memory owner, without loading any project's settings.
pub(crate) fn root(
    settings: &crate::config::Settings,
    cwd: &Path,
) -> Result<(PathBuf, Option<SessionId>), Error> {
    let (_, main) = crate::config::roots(cwd).map_err(|e| refuse(&e.to_string()))?;
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_owned());
    let residents = settings.config.residents.iter();
    let holds = |(_, r): &(&SessionId, &crate::config::Resident)| {
        let folder = r.cwd.canonicalize().unwrap_or_else(|_| r.cwd.clone());
        cwd.starts_with(&folder) || main.starts_with(&folder)
    };
    let found = residents
        .filter(holds)
        .max_by_key(|(_, r)| r.cwd.components().count());
    Ok(match found {
        Some((name, r)) => (r.cwd.clone(), Some(name.clone())),
        None => (main, None),
    })
}

pub(crate) fn dream_sessions_heading() -> &'static str {
    &TEXT.dream_sessions
}

/// The server-created dreaming run, from its schedule input or resident fresh-run job.
pub(crate) fn dreaming(summary: &crate::record::Summary) -> bool {
    summary.dream
        || summary
            .thread
            .as_ref()
            .is_some_and(|t| t.job.as_deref() == Some("dream"))
}

/// A session making a task change, and its name.
type By<'a> = (&'a SessionId, &'a str);

/// A task writer's or closer's session and server ids, followed by its name and machine.
fn who(session: impl fmt::Display, server: &str, name: &str) -> String {
    format!("{session} {server} {name} on {}", machine())
}

/// Admits each note to its session, as from the program `razor`, while that session is open.
async fn send(c: &Controller, notes: Vec<Note>) {
    for (session, text) in notes {
        let Some(session) = open(c, &session).await else {
            continue;
        };
        let name = "razor".into();
        let from = Origin::Program { name };
        if let Err(e) = c.admit(&session, Input::new(from, text)).await {
            eprintln!("razor: a task note to {session}: {e}");
        }
    }
}

/// Session `id` while it is open: a note never reopens a closed one.
async fn open(c: &Controller, id: &str) -> Option<SessionId> {
    let id = SessionId::try_from(id.to_owned()).ok()?;
    c.summary(&id).await.ok().filter(|s| !s.closed).map(|_| id)
}

/// `op` on a thread of its own, off the loop.
async fn blocking<T>(op: impl FnOnce() -> Result<T, Error> + Send + 'static) -> Result<T, Error>
where
    T: Send + 'static,
{
    let done = tokio::task::spawn_blocking(op).await;
    done.map_err(other("waiting for memory's own thread"))?
}

/// `text` with each `{name}` in `with` filled in.
fn fill(text: &str, with: &[(&str, &dyn fmt::Display)]) -> String {
    let mut text = text.to_owned();
    for (name, value) in with {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

/// The entry a ruling cites (`ref`, or the person's latest input), with their original words. Refused unless it is an input from the person or an answer to an ask.
fn cite<'a>(c: &'a Caller, r: Option<EntryId>) -> Result<(&'a Cited, &'a str), Error> {
    match (c.cited.as_ref(), r) {
        (Some(cited @ Cited { said: Some(w), .. }), _) => Ok((cited, w)),
        (Some(&Cited { id, .. }), _) | (None, Some(id)) => Err(refuse(
            &TEXT.not_person.replace("{entry}", &id.0.to_string()),
        )),
        (None, None) => Err(refuse(&TEXT.no_person)),
    }
}

/// Why a memory or task call failed.
#[derive(Debug)]
pub(crate) enum Error {
    /// Something the model should read and act on, in one sentence: input that does not fit,
    /// a ruling that cites no words of the person's, a child's write, an id or number with no
    /// memory or task, a claim held by another session, or git's own words.
    Refused(String),
    /// A file could not be read or written, with what was being done.
    Other {
        source: Box<dyn std::error::Error + Send + Sync>,
        context: &'static str,
    },
}

fn refuse(sentence: &str) -> Error {
    Error::Refused(sentence.to_string())
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
            Error::Refused(sentence) => f.write_str(sentence),
            Error::Other { source, context } => {
                let with: [(_, &dyn fmt::Display); 2] = [("context", context), ("source", source)];
                f.write_str(&fill(&TEXT.other, &with))
            }
        }
    }
}

impl std::error::Error for Error {}
