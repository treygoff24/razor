//! The terminal app is a client of the API server. Everything it holds is a capped cache of the
//! streams, disposable at quit; only the server owns a record.
mod activity;
mod agents;
mod api;
mod audio;
mod bindings;
mod brand;
mod cards;
mod children;
mod clicks;
mod clipboard;
mod commands;
mod composer;
mod data;
mod editor;
mod gauge;
mod goto;
mod image_codec;
mod inputs;
mod keys;
mod markdown;
mod mcp;
mod motion;
mod output;
mod panel;
mod picker;
mod pictures;
mod questions;
mod recap;
mod screen;
mod split;
mod stage;
mod start;
mod status;
mod streams;
mod strip;
mod tasks;
mod text;
mod tools;
mod topbar;
mod transcript;
mod tty;
mod view;
mod writer;

use crate::protocol::{AgentView, Ask, Brand, ClientInput, EntryId, Event as ApiEvent};
use crate::protocol::{Live, MailId, Models, SessionId, State, now};
use api::{Api, Incoming, Line};
use composer::Composer;
use ratatui::layout::Rect;
use std::{collections::BTreeMap, future::Future, io::Write, path::PathBuf, pin::Pin, sync::Arc};
use tokio::{
    sync::{Semaphore, mpsc, oneshot},
    task::JoinHandle,
    time::{Duration, Instant},
};
use transcript::Transcript;

/// A finished operation's effect on the app, applied on the loop between draws.
type Done = Box<dyn FnOnce(&mut App) + Send>;
type Job = Pin<Box<dyn Future<Output = Done> + Send>>;
// The rest of leaving the addressed session.
type Leave = Box<dyn FnOnce(&mut App)>;

/// Renders a session and one opened child through the versioned API, never a record file.
/// Main supplies the socket-bound or remote connection, first input and the Razor home whose
/// machine settings name the audio programs; a session is made on admission. A second Ctrl-C in
/// a row, or a hangup, restores the terminal and quits without stopping a session or its jobs.
pub async fn run(
    server: crate::api::Connection,
    session: Option<SessionId>,
    initial: ClientInput,
    home: PathBuf,
    agents: bool,
) -> Result<(), Error> {
    let session = session.map(|s| s.on(&server.server));
    let (done, mut finished) = mpsc::channel(16);
    // At most 16 lines in flight and 8 MiB of their bytes, or one larger whole entry alone.
    let (lines, mut incoming) = mpsc::channel(16);
    let (api, budget) = (
        Api::new(server.http, server.url, server.key),
        Arc::new(Semaphore::new(api::BYTES)),
    );
    api.stream(None, 0, lines.clone(), budget.clone());
    let mut app = App::new(api, done, lines, budget, server.cwd);
    app.server = server.server;
    // At most eight audio effects wait; a recording's levels skip a draw rather than wait.
    let (sender, mut audio) = mpsc::channel(8);
    #[cfg(target_os = "macos")]
    activity::start(sender.clone());
    app.bindings = bindings::compile(&app.ui.keys)?;
    app.list.kept = home.join("list-width");
    let width = std::fs::read_to_string(&app.list.kept).ok();
    app.list.width = width.and_then(|w| w.trim().parse().ok());
    audio::start(&mut app, home, sender);
    app.emoji = data::load()?;
    composer::emoji::read(&mut app);
    app.model = initial.model.clone().unwrap_or_default();
    app.composer.replace(initial.text.clone());
    app.composer.initial_images(initial.images.clone());
    (app.root, app.focus, app.initial) = (session.clone(), session.clone(), Some(initial));
    picker::request(&mut app, false);
    if let Some(id) = session {
        app.open(id);
    }
    if agents {
        agents::toggle(&mut app);
    }
    let mut recent = agents && app.focus.is_none() && app.composer.is_empty();
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(Error::Terminal)?;
    let mut modes = Some(screen::Modes::enter(screen::QUERY)?);
    // Probe once after the first frame; tmux keeps the answer to itself.
    let mut probe = !app.tmux;
    let (mut kept, mut events) = (None, tty::Events::default());
    let ran = async {
        let terminal = kept.insert(screen::open()?);
        app.gauge_folded = terminal.size().is_ok_and(|size| size.height < 30);
        if !app.composer.is_empty() {
            inputs::send(&mut app);
        }
        let mut last_draw = Instant::now() - motion::FRAME;
        loop {
            if app.leaving {
                return app.quit(finished).await;
            }
            if let Some(editing) = app.editing.take() {
                screen::edit(terminal, &mut app, &mut events, &mut modes, editing).await?;
            }
            stage::remember(&mut app);
            // A frame, a clipboard write or a picture's row is queued only once the terminal has
            // taken everything before it, so the queue holds one of them at most.
            if terminal.backend().writer().idle() {
                clipboard::write(&mut app, terminal.backend_mut())?;
            }
            let idle = terminal.backend().writer().idle();
            let pace = motion::pace(app.cost, app.moving.is_some());
            if app.dirty && idle && (app.immediate || last_draw.elapsed() >= pace) {
                let start = Instant::now();
                screen::draw(terminal, &mut app)?;
                (last_draw, app.cost) = (Instant::now(), start.elapsed());
                if std::mem::take(&mut probe) {
                    let query = app.pictures.query();
                    let out = terminal.backend_mut().writer_mut();
                    out.write_all(query.as_bytes())
                        .and_then(|_| out.flush())
                        .map_err(Error::Terminal)?;
                }
                children::read_visible(&mut app, now());
                output::read_visible(&mut app);
                transcript::read_visible(&mut app);
                pictures::read_visible(&mut app);
                (app.dirty, app.immediate) = (false, false);
            }
            // A picture's next row, which the next frame places.
            if terminal.backend().writer().idle()
                && let Some(row) = app.pictures.row()
            {
                let out = terminal.backend_mut().writer_mut();
                out.write_all(&row).and_then(|_| out.flush()).map_err(Error::Terminal)?;
                app.dirty = true;
            }
            let idle = terminal.backend().writer().idle();
            let wake = app.wake(last_draw, idle);
            tokio::select! {
                event = events.next() => {
                    let Some(event) = event else { return Ok(()) };
                    if keys::event(&mut app, event.map_err(Error::Terminal)?) {
                        return app.quit(finished).await;
                    }
                }
                Some(line) = incoming.recv() => {
                    let snapshot = matches!(&line.line, Line::Event(ApiEvent::Live(Live::Agents { .. })));
                    app.fold_line(line);
                    if snapshot && std::mem::take(&mut recent)
                        && app.focus.is_none() && app.composer.is_empty() && app.edits == 0
                        && app.sending.is_none() && app.start.is_none()
                    {
                        agents::recent(&mut app);
                    }
                },
                _ = hangup.recv() => return app.quit(finished).await,
                Some(done) = audio.recv() => {
                    done(&mut app);
                    app.dirty = true;
                }
                Some(done) = finished.recv() => {
                    app.pending = app.pending.saturating_sub(1);
                    done(&mut app);
                    agents::earlier::sync(&mut app);
                    app.dirty = true;
                }
                written = terminal.backend_mut().writer_mut().drain(), if !idle => {
                    written.map_err(Error::Terminal)?;
                }
                _ = tokio::time::sleep_until(wake) => app.tick(),
            }
        }
    }
    .await;
    drop(audio);
    audio::close(&mut app).await;
    if let Some(terminal) = &mut kept {
        screen::finish(terminal, &mut app, &mut events).await;
    }
    drop(modes);
    if let (Ok(()), Some(line)) = (&ran, app.farewell()) {
        println!("{line}");
    }
    ran
}

/// A terminal failure, or another cause retained with a sentence of context.
#[derive(Debug)]
pub enum Error {
    /// Terminal setup or painting failed; main can offer --print.
    Terminal(std::io::Error),
    /// An HTTP refusal retains its status so background reads never retry a 4xx.
    Refused { status: u16, sentence: String },
    /// An API, data or task failure with its underlying cause.
    Other {
        context: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal(e) => write!(f, "The terminal failed: {e}; use --print to continue."),
            Self::Refused { status, sentence } => write!(f, "API {status}: {sentence}"),
            Self::Other { context, .. } => f.write_str(context),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Terminal(e) => Some(e),
            Self::Refused { .. } => None,
            Self::Other { source, .. } => Some(source.as_ref()),
        }
    }
}
impl Error {
    /// A 4xx: the request itself was refused, so a background read never retries it.
    fn refused(&self) -> bool {
        matches!(self, Self::Refused { status, .. } if (400..500).contains(status))
    }
}
/// Wraps a cause under `context`, the sentence a person reads.
fn fail<E: std::error::Error + Send + Sync + 'static>(context: &str) -> impl FnOnce(E) -> Error {
    let context = context.to_owned();
    move |source| Error::Other {
        context,
        source: Box::new(source),
    }
}
/// An error that is its own cause.
fn error(context: impl Into<String>) -> Error {
    let context = context.into();
    Error::Other {
        source: context.clone().into(),
        context,
    }
}

/// What the app shows. Status rows, transcripts and every list are bounded caches of the
/// streams; drafts and emoji history belong to the API, and the rest is disposable.
#[derive(Default)]
struct App {
    // At most one voice note and one player, and their programs' newest task.
    audio: audio::Audio,
    bindings: bindings::Bindings,
    api: Option<Api>,
    done: Option<mpsc::Sender<Done>>,
    writes: Option<mpsc::Sender<Job>>,
    lines: Option<mpsc::Sender<Incoming>>,
    // Stream lines in flight hold their bytes of this 8 MiB budget until folded.
    budget: Option<Arc<Semaphore>>,
    // Spawned operations not yet applied; at most eight, plus eight queued writes.
    pending: usize,
    // Writes queued so far, and the words the server held as a session's draft as of that count:
    // leaving the session with these same words needs no save.
    queued: u64,
    kept: Option<(u64, SessionId, String)>,
    // The rest of leaving the addressed session, waiting for the server to hold its words.
    parting: Option<Leave>,
    streams: BTreeMap<SessionId, JoinHandle<()>>,
    dirty: bool,
    immediate: bool,
    draft_at: Option<Instant>,
    root: Option<SessionId>,
    focus: Option<SessionId>,
    cwd: PathBuf,
    // The resident a new session is a thread of (its native name); read only while no session is
    // addressed, which `start::aim` alone brings about.
    resident: Option<SessionId>,
    model: String,
    models: Option<Models>,
    models_at: u64,
    picker: Option<picker::Picker>,
    composer: Composer,
    // The session whose saved draft the composer holds, read or made with it: the one it saves to.
    drafted: Option<SessionId>,
    // New agents' unsent words by where each was aimed (server, folder, resident): no session
    // holds them yet, so they wait here for that same destination. At most `start::NEW_DRAFTS`
    // of them, each within the editor's 1 MiB; leaving words that would pass either is refused.
    new_drafts: BTreeMap<start::Destination, Composer>,
    // The draft the stash key set aside, until it brings it back.
    stash: Option<Composer>,
    // What the loop hands the terminal to the person's editor for, between draws.
    editing: Option<editor::Editing>,
    menu: composer::completion::Menu,
    // The emoji table, the open grid, the shortcode strip, and the person's emoji history as
    // last read: at most 32 emoji and their tones.
    emoji: data::Table,
    grid: Option<composer::emoji::Grid>,
    codes: composer::shortcodes::Codes,
    used: crate::protocol::Emoji,
    // The addressed session's skills, at most 1,024, read when a command starts being typed.
    skills: Vec<crate::protocol::Skill>,
    // The person's own draft while an earlier input is recalled into the editor.
    history: Option<(EntryId, Composer)>,
    history_loading: bool,
    transcripts: BTreeMap<SessionId, Transcript>,
    // Only the shown root and focus: at most two responses under the API's 64 MiB cap.
    conversations: BTreeMap<SessionId, children::Conversation>,
    // Wire rows whole, at most 1,024 of them and 8 MiB; a snapshot replaces the map.
    agents: BTreeMap<SessionId, AgentView>,
    agent_bytes: usize,
    servers: Vec<crate::protocol::Server>,
    config: Option<String>,
    brand: Brand,
    // The person's display settings; `ui.motion` is the motion switch every drawer reads.
    ui: crate::protocol::Ui,
    // What the last draw asked for, its cost for the pace, and the person's comings and goings.
    next: motion::Wake,
    cost: Duration,
    presence: motion::Presence,
    // From his return to a session until its recap is asked, that session, the entry to recap
    // after and when it was last looked at. Then the recap's session, that look, its words (empty
    // until they land) and when they landed.
    away: Option<(SessionId, EntryId, u64)>,
    recap: Option<(SessionId, u64, String, u64)>,
    // When the main session's place and headline last changed while attached.
    changed: [Option<u64>; 2],
    lights: tools::rim::Lights,
    // The frame moving, while one does.
    moving: Option<motion::Moving>,
    stage: stage::Stage,
    // The focused folder's branch (empty when detached) and uncommitted files, read by the
    // project route on a focus change and on the focused session's turn ends.
    folder: Option<(SessionId, String, u64)>,
    tasks: tasks::Tasks,
    // The newest 256 deliveries, ordered by their original (at, session, entry).
    delivered: std::collections::VecDeque<crate::protocol::Delivery>,
    inbox: Option<usize>,
    list: agents::List,
    // The new-agent chooser while open; the server a new session starts on (empty for this
    // one), the request id of its creation until it is answered, and the session addressed
    // before, which Escape returns to.
    start: Option<start::Start>,
    server: String,
    request: Option<String>,
    back: Option<SessionId>,
    answering: Option<MailId>,
    shown_ask: Option<Ask>,
    visible_options: std::collections::BTreeSet<usize>,
    // The drawn targets Tab and Ctrl-N cycle, in the cockpit's ring order and without repeats;
    // Enter on an empty composer does what a click on one does.
    targets: Vec<view::Action>,
    selected: Option<view::Action>,
    // The ask the person moved the band to, held by its id; when it closes the band shows the
    // first. A digit or click within 600 ms of the shown ask changing or opening is refused.
    ask: Option<MailId>,
    ask_at: u64,
    // The last option picked and when: the same pick again within 1.5 s is a double press.
    picked: Option<(MailId, u64)>,
    // The ask whose one-row band the person opened to its options, by a click or a first digit.
    ask_open: Option<MailId>,
    notice: Option<(String, u64)>,
    // A ready release is announced once per terminal launch, including after reconnecting.
    release_noticed: bool,
    // A first Ctrl-C and no key or paste since: the notice row says so and the next Ctrl-C quits.
    quitting: bool,
    // `/detach` was typed, or the session `/done` or `/quit` closed has ended: the loop quits as a
    // second Ctrl-C does.
    leaving: bool,
    // The session `/done` or `/quit` closed, until its status row reads it closed, and its place
    // among the list's rows then, for a row the list drops first.
    closing: Option<(SessionId, usize)>,
    panel: Option<panel::Panel>,
    goto: Option<goto::Goto>,
    panel_scroll: usize,
    // The words typed after `/` in the key list, commands or tasks, until Esc ends the search.
    panel_search: Option<String>,
    login: Option<tokio::task::AbortHandle>,
    pane_open: Option<bool>,
    // The archived children are listed in full; the children strip is open.
    archive: bool,
    strip: bool,
    status_scroll: usize,
    // The pane's next draw scrolls to its inbox: after the unread chip or an inbox page.
    to_inbox: bool,
    status_area: Option<Rect>,
    columns: u16,
    // Started inside tmux, which re-sends each cell through its host's width table.
    tmux: bool,
    zone: Option<jiff::tz::TimeZone>,
    gauge_folded: bool,
    // The focused tape's last move: whose, the share it moved from and when (`gauge::tide`).
    tide: Option<(SessionId, f64, u64)>,
    busy_at: Option<u64>,
    // The words of an input being admitted, moved out of the editor at Enter until it is answered;
    // while no answer came, its failure and Enter's posting it again as it was sent.
    sending: Option<Composer>,
    held: Option<(String, Done)>,
    loading_images: usize,
    output_loading: bool,
    copying: bool,
    // An encoded copy for the frame loop to write, and its toast.
    clipboard: Option<(String, String)>,
    selection: Option<transcript::select::Selection>,
    // The last click's time, cell and count, for double and triple clicks.
    clicks: (u64, (u16, u16), u8),
    // At most one link per visible cell; destinations share the parsed paragraph's bytes.
    links: Vec<(Rect, Arc<str>)>,
    painted: screen::Painted,
    pictures: pictures::Pictures,
    hits: Vec<view::Hit>,
    // Launch choices ride on the first admitted input, never on a bare launch.
    initial: Option<ClientInput>,
    revision: u64,
    // One per status row: last activity, retry-after (MAX on success/refusal), in flight, failure said.
    previews: BTreeMap<SessionId, (u64, u64, bool, bool)>,
    edits: u64,
    // The tab's title as last written.
    title: String,
    program: screen::ProgramStatus,
}

impl App {
    fn new(
        api: Api,
        done: mpsc::Sender<Done>,
        lines: mpsc::Sender<Incoming>,
        budget: Arc<Semaphore>,
        cwd: PathBuf,
    ) -> Self {
        let (writes, mut queue) = mpsc::channel::<Job>(8);
        let finished = done.clone();
        // Drafts, admissions and opens run one at a time in order, so an earlier draft never
        // lands after the admission that cleared it; past quit, their effects go unapplied.
        tokio::spawn(async move {
            while let Some(job) = queue.recv().await {
                let _ = finished.send(job.await).await;
            }
        });
        Self {
            api: Some(api),
            done: Some(done),
            writes: Some(writes),
            lines: Some(lines),
            budget: Some(budget),
            cwd,
            zone: Some(jiff::tz::TimeZone::system()),
            tmux: std::env::var_os("TMUX").is_some(),
            dirty: true,
            presence: motion::Presence::new(now()),
            ..Self::default()
        }
    }

    /// Runs `work` beside the loop and applies its effect when done; refused past eight.
    fn spawn<W, F, G>(&mut self, work: W) -> bool
    where
        W: FnOnce(Api) -> F,
        F: Future<Output = G> + Send + 'static,
        G: FnOnce(&mut App) + Send + 'static,
    {
        if self.pending >= 8 {
            self.say("Eight API operations are pending; wait for an answer before trying again.");
            return false;
        }
        let (Some(api), Some(done)) = (self.api.clone(), self.done.clone()) else {
            return false;
        };
        let work = work(api);
        self.pending += 1;
        tokio::spawn(async move { done.send(Box::new(work.await)).await });
        true
    }

    /// Queues `work` behind every earlier draft, admission and open; refused past eight.
    fn write<W, F, G>(&mut self, work: W) -> bool
    where
        W: FnOnce(Api) -> F,
        F: Future<Output = G> + Send + 'static,
        G: FnOnce(&mut App) + Send + 'static,
    {
        let Some(work) = self.api.clone().map(work) else {
            return false;
        };
        let job: Job = Box::pin(async move { Box::new(work.await) as Done });
        let queued = self
            .writes
            .as_ref()
            .is_some_and(|w| w.try_send(job).is_ok());
        match queued {
            true => (self.pending, self.queued) = (self.pending + 1, self.queued + 1),
            false => {
                self.say("Eight draft or input writes are pending; keep the unsent words and wait.")
            }
        }
        queued
    }

    /// The composer holds the addressed session's saved draft, or no session is addressed.
    fn drafting(&self) -> bool {
        self.drafted == self.focus
    }

    /// Whether leaving the addressed session now would lose or misplace words: an admission or
    /// note is pending, or words were typed before its saved draft was read, read again now.
    fn waiting(&mut self) -> bool {
        if self.sending.is_some() || self.loading_images > 0 || audio::unsent(self) {
            return true;
        }
        let unread = (self.focus.clone()).filter(|_| !self.drafting() && !self.composer.is_empty());
        unread.clone().into_iter().for_each(|id| self.open(id));
        unread.is_some()
    }

    /// Saves the addressed session's unsent words, once its saved draft is among them. `leave`,
    /// the rest of leaving the session, runs once the server holds them: at once if it already
    /// does or there are none, never if the save is refused or fails, which says why.
    fn save(&mut self, leave: Option<Leave>) {
        // A leaving save's answer saves anything typed since; a second departure waits for none.
        if self.parting.is_some() {
            return;
        }
        let id = (self.focus.clone()).filter(|_| self.sending.is_none() && self.drafting());
        let kept = id.map(|id| (self.queued, id, self.composer.text.clone()));
        if let Some((at, id, text)) = kept.filter(|kept| self.kept.as_ref() != Some(kept)) {
            let leaving = leave.is_some() && !self.composer.is_empty();
            let queued = self.write(|api| async move {
                let saved = api.draft(&id, Some(text.clone())).await;
                move |app: &mut App| match saved {
                    Ok(_) => {
                        app.kept = Some((at + 1, id, text));
                        if let Some(leave) = app.parting.take_if(|_| leaving) {
                            app.save(Some(leave));
                        }
                    }
                    Err(e) => {
                        app.parting.take_if(|_| leaving);
                        app.say(&e.to_string());
                    }
                }
            });
            if leaving {
                self.parting = leave.filter(|_| queued);
                return;
            }
        }
        leave.into_iter().for_each(|leave| leave(self));
    }

    /// Runs `then` once the addressed session has been left, or now if none is being left.
    fn after(&mut self, then: impl FnOnce(&mut App) + 'static) {
        match self.parting.take() {
            Some(leave) => {
                self.parting = Some(Box::new(|app: &mut App| {
                    leave(app);
                    then(app);
                }))
            }
            None => then(self),
        }
    }

    /// Ctrl-C: waits up to 30 seconds for the last draft save, then quits. Behind a pending
    /// admission or a draft not yet read, words typed since go after the server's draft: the sent
    /// words, if refused. A held send's words go ahead of them whether or not it landed, after
    /// only a draft not yet read. Effects still `finished` are dropped unapplied, freeing what
    /// they hold: a read page holds the one page permit an open queued ahead of the save waits for.
    async fn quit(&mut self, finished: mpsc::Receiver<Done>) -> Result<(), Error> {
        drop(finished);
        let held = self.held.as_ref().and(self.sending.as_ref());
        let pending = (self.sending.is_some() && held.is_none()) || !self.drafting();
        let mut words = vec![held.map_or("", |h| h.text.as_str()), &self.composer.text];
        words.retain(|w| !w.is_empty());
        let mut text = words.join("\n");
        let (api, writes, saved) = (self.api.clone(), self.writes.take(), self.focus.clone());
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let job: Job = Box::pin(async move {
            let result = match (saved, api) {
                (Some(_), Some(_)) if pending && text.is_empty() => Ok(()),
                (Some(id), Some(api)) => {
                    async {
                        let kept = match pending {
                            true => api.draft(&id, None).await?,
                            false => String::new(),
                        };
                        if !kept.is_empty() {
                            text = format!("{kept}\n{text}");
                        }
                        api.draft(&id, Some(text)).await.map(drop)
                    }
                    .await
                }
                _ => Ok(()),
            };
            let _ = tx.send(result);
            Box::new(|_: &mut App| {}) as Done
        });
        let closed = "The draft writer closed; keep the unsent words before quitting.";
        let saving = async {
            let writes = writes.ok_or_else(|| error(closed))?;
            writes.send(job).await.map_err(|_| error(closed))?;
            rx.await.map_err(fail(closed))?
        };
        let late = "Saving the draft timed out; check its saved words before sending again.";
        let saved = tokio::time::timeout(Duration::from_secs(30), saving).await;
        saved.map_err(fail(late))?
    }

    fn agent_row(&self, id: &SessionId) -> Option<&AgentView> {
        self.agents
            .get(id)
            .or_else(|| self.conversations.get(id)?.detail.as_ref().map(|d| &d.row))
            .or_else(|| {
                self.conversations
                    .values()
                    .filter_map(|c| c.detail.as_ref())
                    .flat_map(|d| &d.children)
                    .find(|a| &a.session == id)
            })
            .or_else(|| self.list.earlier.row(id))
    }

    fn agent_children<'a>(&'a self, id: &SessionId) -> impl Iterator<Item = &'a AgentView> {
        self.agents
            .values()
            .filter(move |a| a.parent.as_ref() == Some(id))
            .chain(
                self.conversations
                    .get(id)
                    .and_then(|c| c.detail.as_ref())
                    .into_iter()
                    .flat_map(|d| &d.children)
                    .filter(|a| !self.agents.contains_key(&a.session)),
            )
    }

    /// The line left on the normal screen once the app has quit: the session shown, the addressed
    /// one beside the list, else the root, ended, or it stays open on the server and `--resume`
    /// reopens it; none before a session started.
    fn farewell(&self) -> Option<String> {
        let id = [&self.root, &self.focus][usize::from(self.list.open)].as_ref()?;
        let row = self.agent_row(id);
        let name = row.map_or("", |a| a.name.as_str());
        let name = if name.is_empty() { id.as_str() } else { name };
        let ended = row.is_some_and(|a| a.state == State::Done || a.settled.is_some());
        Some(match ended {
            true => format!("Session ended: {name}."),
            false => format!("Left {name}; it stays open. Reopen it with razor --resume {id}"),
        })
    }

    fn tick(&mut self) {
        self.dirty = true;
        if self.draft_at.is_some_and(|at| at <= Instant::now()) && self.pending < 8 {
            self.draft_at = None;
            self.save(None);
        }
        goto::tick(self);
    }

    /// Marks the editor changed; its words are saved half a second after the last change.
    fn edited(&mut self) {
        self.dismiss_suggestion();
        self.edits += 1;
        self.draft_at = Some(Instant::now() + Duration::from_millis(500));
        composer::completion::edited(self, now());
    }

    /// Leaves browsing click targets, redrawing the mark; false when nothing was selected.
    fn unselect(&mut self) -> bool {
        let selected = self.selected.take().is_some();
        self.revision = self.revision.wrapping_add(selected.into());
        selected
    }

    // The drawn screen's next change, paced; while anything works, the clocks' seconds, of the wall
    // and of the focused turn, unless the overview covers them; a draft save that `tick` can
    // queue (past eight pending, a finished one wakes the loop) and the notice's end; else the
    // minute.
    fn wake(&self, last_draw: Instant, drawable: bool) -> Instant {
        let now = now();
        let working =
            !agents::covers(self, now) && self.agents.values().any(|a| a.state == State::Working);
        let seconds = [Some(0), self.busy_at].map(|since| {
            since
                .filter(|_| working)
                .map(|since| now + 1000 - now.saturating_sub(since) % 1000)
        });
        // Unfocused, the drawn screen asks at most once a second; data still draws on arrival.
        let floor = |at: u64| at.max(now + if self.presence.focused { 0 } else { 1000 });
        let asks = [self.next.get().map(floor), seconds[0], seconds[1]];
        let at = (asks.into_iter().flatten().min()).unwrap_or(now + 60_000 - now % 60_000);
        let moving = self.moving.is_some();
        let mut wake = motion::deadline(at, now, last_draw, self.cost, moving);
        // A frame waiting for the terminal to take the last is drawn once it has.
        if self.dirty && drawable {
            wake = wake.min(last_draw + motion::pace(self.cost, moving));
        }
        let search = self.goto.as_ref().and_then(|g| g.due);
        let save = self.draft_at.filter(|_| self.pending < 8);
        wake = [save, search].into_iter().flatten().fold(wake, Ord::min);
        if let Some((_, until)) = self.notice.as_ref().filter(|(_, until)| *until > now) {
            wake = wake.min(Instant::now() + Duration::from_millis(until - now));
        }
        // A picture's row unanswered is given up on when due.
        self.pictures
            .due()
            .filter(|_| drawable)
            .map_or(wake, |due| wake.min(due))
    }

    /// The model and effort the next input would carry: a staged choice, the agent's row,
    /// then the configured defaults.
    fn choice(&self) -> (&str, Option<crate::protocol::Effort>) {
        let row = self.row();
        let initial = self.initial.as_ref();
        let model = initial
            .and_then(|v| v.model.as_deref())
            .or_else(|| row.map(|v| v.model.as_str()));
        let model = model.or_else(|| self.models.as_ref().map(|v| v.model.as_str()));
        let effort = initial
            .and_then(|v| v.effort)
            .or_else(|| row.map(|v| v.effort));
        (
            model.unwrap_or(&self.model),
            effort.or_else(|| self.models.as_ref().map(|v| v.effort)),
        )
    }
    fn speed(&self) -> crate::protocol::Speed {
        self.initial
            .as_ref()
            .and_then(|i| i.speed)
            .or_else(|| self.row().map(|r| r.speed))
            .or_else(|| self.models.as_ref().map(|m| m.speed))
            .unwrap_or_default()
    }
    /// The addressed session's status row.
    fn row(&self) -> Option<&AgentView> {
        self.agent_row(self.focus.as_ref()?)
    }
    fn split(&self) -> bool {
        self.focus != self.root
    }
    /// A child's split in full screen, where Esc goes back; with the agents list open, at any
    /// width, the addressed session is the conversation itself, spoken to and stopped as in full screen.
    fn split_view(&self) -> bool {
        self.split() && !self.list.open
    }
    /// A narrow phone uses half blocks; in tmux auto does too because its width table may not
    /// know octants. An explicit octant setting overrides the tmux default.
    fn octant(&self) -> bool {
        use crate::protocol::Blocks;
        self.columns >= view::PHONE
            && (self.ui.blocks == Blocks::Octant || self.ui.blocks == Blocks::Auto && !self.tmux)
    }
    fn transcript(&self) -> Option<&Transcript> {
        self.transcripts.get(self.focus.as_ref()?)
    }
    fn transcript_mut(&mut self) -> Option<&mut Transcript> {
        self.transcripts.get_mut(self.focus.as_ref()?)
    }
    /// The child whose card Tab or Ctrl-N selected.
    fn selected_child(&self) -> Option<&SessionId> {
        match &self.selected {
            Some(view::Action::Child(id)) => Some(id),
            _ => None,
        }
    }
    /// A toast for 2.6 seconds, Loom's `TOAST_MS`.
    fn say(&mut self, text: &str) {
        self.notice = Some((text::clip(text, 512), now() + 2600));
    }
    /// Every open ask, the addressed session's first, oldest first within a session.
    fn asks(&self) -> Vec<&Ask> {
        let mine = |a: &&Ask| self.focus.as_ref() == Some(&a.id.0);
        let all = self.agents.values().flat_map(|row| &row.asks);
        let (mut asks, mut rest): (Vec<_>, Vec<_>) = all.partition(mine);
        rest.sort_by_key(|a| a.at);
        asks.extend(rest);
        asks
    }
    /// The shown ask: the one being answered in words, else the one the person moved to, while
    /// it is open; else the first.
    fn question(&self) -> Option<&Ask> {
        let asks = self.asks();
        let mut held = [&self.answering, &self.ask].into_iter().flatten();
        let held = held.find_map(|id| asks.iter().find(|a| &a.id == id));
        held.or(asks.first()).copied()
    }
}
