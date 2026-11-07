//! Memory and tasks (docs/design/memory.md): the startup page and the
//! `memory` and `task` actions, on Razor's own files, and the rules for their writes. Tasks
//! share memory's files, page and project; a project's tasks may be shared through git.

mod claims;
mod files;
mod share;
mod tasks;

use crate::controller::Controller;
use crate::profiles::machine;
use crate::protocol::{Body, EntryId, Input, Origin, SessionId};
use crate::record::{Record, closing_text};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::AtomicBool;
use tasks::{Edit, Listing};

/// The most of the question a ruling answers that its page line shows, in bytes.
const ASKED: usize = 300;
/// The most bytes of a state note, written or shown.
const STATE: usize = 1500;

#[derive(Deserialize)]
struct Text {
    state: String,
    rulings: String,
    known: String,
    no_state: String,
    none: String,
    more: String,
    until: String,
    input: String,
    child: String,
    thread: String,
    too_long: String,
    state_long: String,
    state_all: String,
    no_text: String,
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
    task_remote: String,
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
    conflict: String,
    changed: String,
    settings: String,
}

static TEXT: LazyLock<Text> = LazyLock::new(|| {
    // Invariant: the file is compiled into the binary, so it parses in every run of a build or
    // in none, and the first memory call of any flow shows which.
    toml::from_str(include_str!("text.toml")).expect("src/memory/text.toml parses")
});

/// Memory and tasks for one server, under `<home>/memory` and `<home>/tasks`. Between calls it
/// holds only a lock, which every task change and sync takes; reads take none, since every file
/// is replaced by one rename.
pub(crate) struct Memory {
    home: PathBuf,
    lock: tokio::sync::Mutex<()>,
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
    said: Option<Said>,
}

/// The person's words in an entry, and the question they answer: an ask's, or the text that
/// closed the turn before; "" when none.
struct Said {
    words: String,
    asked: String,
}

/// The entry `id` of `record` as a ruling may cite it.
pub(crate) async fn cited(record: &Record, id: EntryId) -> Option<Cited> {
    let none = Some(Cited { id, said: None });
    let Body::Input(input) = record.entry(id).await.ok()?.body else {
        return none;
    };
    let mut asked = Vec::new();
    match input.from {
        Origin::Answer { ask } => match record.entry(ask).await.map(|e| e.body) {
            Ok(Body::Call { input: i, .. }) => asked.extend(i["text"].as_str().map(Into::into)),
            Ok(Body::Question { text, .. }) => asked.push(text),
            _ => {}
        },
        // Back past entries outside any turn to the last turn's end, then its closing text.
        Origin::Person => {
            let mut ended = false;
            for at in (1..id.0).rev() {
                match (record.entry(EntryId(at)).await.map(|e| e.body), ended) {
                    (Ok(Body::TurnEnd { .. }), false) => ended = true,
                    (Ok(Body::Text { text }), true) => {
                        asked.insert(0, crate::record::reply(&text).text.to_owned())
                    }
                    (Ok(Body::Thinking { .. }), true) | (Ok(Body::Job(_)), false) => {}
                    _ => break,
                }
            }
        }
        _ => return none,
    }
    let (words, asked) = (input.text, closing_text(&asked));
    let said = Some(Said { words, asked });
    Some(Cited { id, said })
}

/// One `memory` call's input.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Call {
    Search { query: String, limit: Option<usize> },
    Get { id: String },
    Write(Write),
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
#[derive(Deserialize)]
struct Write {
    kind: Kind,
    #[serde(default)]
    text: String,
    /// One line saying what a fact or note is, for the startup page.
    description: Option<String>,
    #[serde(default)]
    all: bool,
    /// A date, `2026-12-31`, through which it applies.
    expires: Option<String>,
    replaces: Option<String>,
    #[serde(rename = "ref")]
    cite: Option<EntryId>,
    /// The entry the memory comes from: never the model's to set.
    #[serde(skip)]
    source: String,
}

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Ruling,
    Fact,
    Note,
    State,
}

/// Where a session keeps memory and tasks, found at each call from the person's settings: the
/// folder of the resident whose folder holds the session's (the closest when several do), else
/// its project's main checkout.
struct Project {
    root: PathBuf,
    memory: files::Folders,
    tasks: PathBuf,
    /// The resident whose folder it is: only its home session writes the state note.
    resident: Option<SessionId>,
    /// The remote the project's private settings name, which its tasks are shared through, or
    /// why those settings do not read.
    remote: Result<Option<String>, String>,
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
        let (home, lock, owed) = (home.into(), tokio::sync::Mutex::new(()), true.into());
        Memory { home, lock, owed }
    }

    /// Whether the folder's state note belongs to a different resident session.
    pub(crate) async fn state_owner(
        &self,
        c: &Controller,
        cwd: &Path,
        session: &SessionId,
    ) -> Result<Option<SessionId>, Error> {
        let session = session.clone();
        self.files(c, cwd, move |p| Ok(p.resident.filter(|r| r != &session)))
            .await
    }

    /// The startup page for `session` in `cwd`: the project's state note (at most 1,500 bytes);
    /// the rulings, standing ones first, newest first, at most 20 lines in 4,000 bytes, each the
    /// person's words (of one in Loom's form, those it quotes, after any question they answer)
    /// and its id; a line for each fact and note, newest first, at most 24 in 2,400 bytes; then
    /// the tasks it holds and those due within a week, and how many are ready, or, for a shared
    /// project with no open task here, that `task list` reads them. A line that does
    /// not fit is passed over, not cut. It never fails: a page that cannot be read is one
    /// sentence saying why. It never fetches.
    pub(crate) async fn page(&self, c: &Controller, cwd: &Path, session: &SessionId) -> String {
        let session = session.to_string();
        let read = self.files(c, cwd, move |p| {
            let shared = matches!(p.remote, Ok(Some(_)));
            let tasks = tasks::page(&p.tasks, &session, &p.server, shared)?;
            Ok((files::page(&p.memory)?, tasks))
        });
        let ((state, rulings, known), tasks) = match read.await {
            Ok(read) => read,
            Err(e) => return e.to_string(),
        };
        let t = &*TEXT;
        let none = |s: String| if s.is_empty() { t.none.clone() } else { s };
        let state = state.as_deref().map(|s| &s[..s.floor_char_boundary(STATE)]);
        let state = state.unwrap_or(&t.no_state);
        let (rulings, known) = (none(rulings), none(known));
        let memory = format!("{}\n\n{state}\n\n{}\n\n{rulings}", t.state, t.rulings);
        let page = format!("{memory}\n\n{}\n\n{known}\n\n{tasks}", t.known);
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
        if tool == "task" {
            return self.task(c, &at, input).await;
        }
        let call = serde_json::from_value(input)
            .map_err(|e| refuse(&TEXT.input.replace("{why}", &e.to_string())))?;
        match call {
            Call::Search { query: q, limit } => {
                let limit = limit.unwrap_or(8).clamp(1, 20);
                let search = move |p: Project| files::search(&p.memory, &q, limit);
                self.files(c, at.cwd, search).await
            }
            Call::Get { id } => {
                let get = move |p: Project| files::get(&p.memory, &id);
                self.files(c, at.cwd, get).await
            }
            Call::Write(write) => self.write(c, &at, write).await,
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
        let id = id.to_owned();
        let forget = move |p: Project| files::retire(&p.memory, &id, true);
        self.files(c, cwd, forget).await
    }

    /// Every fact's and note's line, newest first, at most 200 in 16 KiB: the page's list,
    /// longer, for a resident's night.
    pub(crate) async fn index(&self, c: &Controller, cwd: &Path) -> String {
        let index = |p: Project| files::known(&p.memory, (200, 16 << 10));
        let index = self.files(c, cwd, index).await;
        index.unwrap_or_else(|e| e.to_string())
    }

    /// The project's tasks for the person's clients, as `task list` answers them.
    pub(crate) async fn tasks(&self, c: &Controller, cwd: &Path, filter: Option<String>) -> Answer {
        let l = Listing {
            filter,
            ..Listing::default()
        };
        self.read(c, cwd, move |dir| tasks::list(dir, &l)).await
    }

    /// A `task` call: a child may only claim or comment on an existing task; a list or a get
    /// reads the project's tasks.
    async fn task(&self, c: &Controller, at: &Caller<'_>, input: Value) -> Answer {
        if at.child && input["action"] == "write" {
            let allowed = |k: &String| matches!(k.as_str(), "action" | "id" | "claim" | "comment");
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
                let parent = writer.child.as_ref().map(|child| &child.parent);
                if edit.claim.is_some() && parent.is_some_and(|p| p.address().is_some()) {
                    return Err(refuse(&TEXT.task_remote));
                }
                self.edit(c, at.cwd, (at.session, &writer.name), edit).await
            }
            TaskCall::List(l) => self.read(c, at.cwd, move |dir| tasks::list(dir, &l)).await,
            TaskCall::Get { id } => self.read(c, at.cwd, move |dir| tasks::get(dir, id)).await,
        }
    }

    /// `op` on `cwd`'s tasks, first brought up to date when they are shared and the last fetch
    /// is over 30 seconds old; a remote out of reach leaves them as they are.
    async fn read<F>(&self, c: &Controller, cwd: &Path, op: F) -> Answer
    where
        F: FnOnce(&Path) -> Answer + Send + 'static,
    {
        let p = self.files(c, cwd, Ok).await?;
        if matches!(p.remote, Ok(Some(_))) && share::stale(&p.tasks).await {
            let held = self.lock.lock().await;
            let (_, notes) = self.locked(&p, None).await;
            drop(held);
            send(c, notes).await;
        }
        blocking(move || op(&p.tasks)).await
    }

    /// One change, `edit`, to `cwd`'s tasks by session `by.0`, named `by.1`. On a shared project
    /// it is on the remote before it answers. The notes it causes go out once the lock is
    /// released, and a claim becomes the claimant's headline.
    async fn edit(&self, c: &Controller, cwd: &Path, by: By<'_>, edit: Edit) -> Answer {
        let held = self.lock.lock().await;
        let p = self.files(c, cwd, Ok).await?;
        let (session, name) = by;
        let who = format!("{session} {} {name} on {}", p.server, machine());
        let (answer, notes) = self.locked(&p, Some((who, edit))).await;
        drop(held);
        send(c, notes).await;
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
    async fn locked(&self, p: &Project, change: Option<(String, Edit)>) -> (Answer, Vec<Note>) {
        let (dir, server) = (p.tasks.clone(), p.server.clone());
        let at = dir.clone();
        let before = blocking(move || tasks::scan(&at, false)).await;
        let answer = async {
            let mut last: Answer = Ok(Value::Null);
            for round in 0..3 {
                // The push checks a write against the remote's latest, so fetching first only
                // saves a refused push.
                let fetch = round > 0 || share::stale(&dir).await;
                let url = match &p.remote {
                    Ok(Some(name)) => Some(share::fresh(&p.root, &dir, name, fetch).await?),
                    Ok(None) => None,
                    Err(why) => return Err(refuse(&fill(&TEXT.settings, &[("why", why)]))),
                };
                let Some((who, edit)) = change.clone() else {
                    break;
                };
                let by = tasks::shown(&who).to_owned();
                let (at, server) = (dir.clone(), server.clone());
                let done = blocking(move || tasks::apply(&at, &server, &who, &edit)).await;
                match (url, done) {
                    // A refusal stands only against the remote's latest.
                    (Some(_), Err(_)) if !fetch => {}
                    (Some(url), Ok(done)) => {
                        let subject = format!("task {}, by {by}", done["id"]);
                        last = share::push(&dir, &url, &subject).await?.map(|()| done);
                        if last.is_ok() {
                            break;
                        }
                    }
                    (_, done) => return done,
                }
            }
            last
        };
        let answer = answer.await;
        let closes = blocking(move || tasks::closes(&dir, &server, &before.unwrap_or_default()));
        (answer, closes.await.unwrap_or_default())
    }

    /// A write, a replacement or a retirement, after the rules, all checked before anything is
    /// retired: a ruling's text holds the person's words from the record, a fact or note has a
    /// description, every memory names the entry it comes from, the composed text is at most 16
    /// KiB, and a state note replaces the project's last, whatever `replaces` names, and in a
    /// resident's folder is its home session's.
    async fn write(&self, controller: &Controller, c: &Caller<'_>, mut w: Write) -> Answer {
        let t = &*TEXT;
        if c.child {
            return Err(refuse(&t.child));
        }
        match w.kind {
            Kind::State if w.all => return Err(refuse(&t.state_all)),
            Kind::State if w.text.len() > STATE => return Err(refuse(&t.state_long)),
            Kind::Fact | Kind::Note if !w.text.is_empty() => {
                let line = w.description.as_deref().unwrap_or_default();
                if line.trim().is_empty() || line.len() > 200 || line.contains(['\n', '\r']) {
                    return Err(refuse(&t.description));
                }
            }
            // A state note replaces the last, so `replaces` names nothing else to retire.
            Kind::State => (w.description, w.replaces) = (None, None),
            _ => w.description = None,
        }
        let date = |d: String| d.parse::<jiff::civil::Date>().map(|d| d.to_string());
        let expires = w.expires.map(date).transpose();
        w.expires = expires.map_err(|_| refuse(&t.expires))?;
        w.source = format!("razor:{}:l-{}", c.session, c.at.0);
        let ruled = (w.kind == Kind::Ruling)
            .then(|| cite(c, w.cite))
            .transpose()?;
        if let Some((cited, Said { words, asked })) = ruled.filter(|_| !w.text.is_empty()) {
            // The person's words and the question they answer, verbatim, then the agent's.
            w.source = format!("razor:{}:l-{}", c.session, cited.0);
            let q = format!("the agent asked: \"{asked}\"\n\n");
            let q = if asked.is_empty() { "" } else { &q };
            let said = format!("{q}the human said ({}): \"{words}\"\n\n", w.source);
            w.text = format!("{said}agent's reading: {}", w.text);
        }
        if w.text.len() > 16 * 1024 {
            return Err(refuse(&t.too_long));
        }
        let session = c.session.clone();
        self.files(controller, c.cwd, move |p| {
            if w.kind == Kind::State
                && let Some(home) = p.resident.filter(|r| *r != session)
            {
                let state = files::state(&p.memory).unwrap_or_else(|| TEXT.no_state.clone());
                let (thread, heading) = (&TEXT.thread, &TEXT.state);
                return Err(refuse(&format!("{thread} {home}.\n\n{heading}\n\n{state}")));
            }
            match (&w.replaces, w.text.is_empty()) {
                (Some(old), true) => files::retire(&p.memory, old, false),
                (None, true) => Err(refuse(&TEXT.no_text)),
                _ => files::write(&p.memory, w),
            }
        })
        .await
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
    let (_, main) = crate::config::roots(cwd).map_err(|e| refuse(&e.to_string()))?;
    let residents = settings.config.residents.iter();
    let holds = |(_, r): &(&SessionId, &crate::config::Resident)| {
        cwd.starts_with(&r.cwd) || main.starts_with(&r.cwd)
    };
    let found = residents
        .filter(holds)
        .max_by_key(|(_, r)| r.cwd.components().count());
    let (root, resident) = match found {
        Some((name, r)) => (r.cwd.clone(), Some(name.clone())),
        None => (main, None),
    };
    // A project's own settings that do not read add errors past the machine's.
    let remote = settings.project(&root).map_err(|e| e.to_string());
    let remote = remote.and_then(|s| match s.errors.len() > settings.errors.len() {
        true => Err(s.errors.join(" ")),
        false => Ok(s.config.tasks.remote),
    });
    let server = std::fs::read_to_string(home.join("server.id")).unwrap_or_default();
    Ok(Project {
        memory: files::folders(&home.join("memory"), &root),
        tasks: home.join("tasks").join(tasks::folder(&root)),
        server: server.trim().to_owned(),
        root,
        resident,
        remote,
    })
}

/// A session making a task change, and its name.
type By<'a> = (&'a SessionId, &'a str);

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

/// The entry a ruling cites (`ref`, or the person's latest input), with their words and the
/// question they answer. Refused unless it is an input from the person or an answer to an ask.
fn cite<'a>(c: &'a Caller, r: Option<EntryId>) -> Result<(EntryId, &'a Said), Error> {
    match (c.cited.as_ref(), r) {
        (Some(Cited { id, said: Some(w) }), _) => Ok((*id, w)),
        (Some(&Cited { id, .. }), _) | (None, Some(id)) => Err(refuse(
            &TEXT.not_person.replace("{entry}", &id.0.to_string()),
        )),
        (None, None) => Err(refuse(&TEXT.no_person)),
    }
}

/// A ruling's words on one line: in a body in Loom's form, `the human said (<where>):
/// "<words>"` and then the agent's reading, the quoted words, after the question they answer
/// when `the agent asked: "<question>"` comes first; any other body, as written.
fn words(body: &str) -> String {
    let line = |words: &str| words.split_whitespace().collect::<Vec<_>>().join(" ");
    let asked = body
        .strip_prefix("the agent asked: \"")
        .and_then(|r| r.split_once("\"\n\nthe human said ("));
    let rest = asked.map_or(body.strip_prefix("the human said ("), |(_, r)| Some(r));
    let rest = rest.and_then(|r| r.split_once("): \""));
    let quoted = rest.and_then(|(_, r)| r.split_once("\"\n\nagent's reading:"));
    let said = line(quoted.map_or(body, |(words, _)| words));
    match asked {
        Some((asked, _)) => format!("Asked \"{}\", answered \"{said}\"", end(&line(asked))),
        None => said,
    }
}

/// The question `q`, or past `ASKED` bytes "…" and its words from the first that starts in its
/// last `ASKED` bytes: the page passes over a line that does not fit, and a question sits last.
fn end(q: &str) -> String {
    let Some(cut) = q.len().checked_sub(ASKED).filter(|&cut| cut > 0) else {
        return q.to_string();
    };
    let from = match q.as_bytes()[cut - 1..].iter().position(|&b| b == b' ') {
        Some(space) => cut + space,
        None => q.ceil_char_boundary(cut),
    };
    format!("…{}", &q[from..])
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
