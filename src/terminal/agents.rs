//! The agents view: the list beside the cockpit and the full-screen overview, their keys and
//! commands; `sections` renders the server-resolved sessions the list shares with the status
//! pane, and `overview` draws the same layout over the whole screen.
mod groups;
mod overview;
mod sections;
use super::view::{self, Action, Hit, Question};
use super::{App, keys, status, text::*};
use crate::protocol::views::{Layout, SectionKind, ViewsStatus};
use crate::protocol::{SessionId, now};
use crossterm::event::{KeyCode as K, KeyEvent};
pub(super) use groups::edit;
pub(super) use overview::{covers, draw as overlay};
use ratatui::{buffer::Buffer, layout::Rect};
pub(super) use sections::{GUTTER, addressed, gutter, mark, rows, section_rows};
use std::collections::BTreeSet;
/// Disposable selection, folding and layouts; layouts are bounded to 8 MiB by the server.
#[derive(Default)]
pub(super) struct List {
    pub open: bool,
    pub keys: bool,
    pub area: Option<Rect>,
    changed: u64,
    pub selected: Option<Action>,
    pub view: Option<String>,
    status: Option<ViewsStatus>,
    pub scroll: usize,
    reveal: bool,
    /// The overview while it is shown, with its rows' scroll; it owns the keys meanwhile.
    pub overview: Option<usize>,
    /// The screen the overview last drew on; a new size brings the selection back into view.
    pub sized: Rect,
    /// Headings and children folded from their default (open), and Earlier, finished children's
    /// lines and the servers and residents lines opened (closed); held while their layout holds
    /// them, the servers and residents lines always.
    pub toggled: BTreeSet<String>,
    pub rows: Vec<Action>,
    /// The list's columns as the person last dragged or keyed them, kept in the home's
    /// `list-width`; none, a third of the screen. Set while the edge is held.
    pub width: Option<u16>,
    pub dragging: bool,
    pub kept: std::path::PathBuf,
}
impl List {
    pub fn received(&mut self, mut status: ViewsStatus) {
        if status.error.is_some() {
            self.status = Some(status);
            return;
        }
        if let Some(old) = &mut self.status {
            old.layouts.retain(|l| {
                status.names.contains(&l.name)
                    && !status.layouts.iter().any(|new| new.name == l.name)
            });
            status.layouts.append(&mut old.layouts);
        }
        if (self.view.as_ref()).is_none_or(|name| !status.names.contains(name)) {
            self.view = status.names.first().cloned();
        }
        let sections = || status.layouts.iter().flat_map(|l| &l.sections);
        let rows = || sections().flat_map(|s| &s.sessions);
        self.toggled.retain(|id| {
            let session = id.strip_prefix("finished/").unwrap_or(id);
            let header = ["servers", "residents"].contains(&id.as_str());
            header
                || sections().any(|s| &s.id == id)
                || rows().any(|r| r.session.as_str() == session)
        });
        self.status = Some(status);
    }
    pub(super) fn layout(&self) -> Option<&Layout> {
        let layouts = &self.status.as_ref()?.layouts;
        layouts.iter().find(|l| Some(&l.name) == self.view.as_ref())
    }
}
/// Opens the list with the addressed session highlighted, or closes it to full screen.
pub(super) fn toggle(app: &mut App) {
    if app.list.open && (app.sending.is_some() || super::audio::unsent(app)) {
        return app.say("A note or input is pending; wait before entering a session.");
    }
    (app.list.open, app.list.overview) = (!app.list.open, None);
    app.list.changed = now();
    app.list.reveal = true;
    app.list.keys = app.list.open;
    app.list.selected = app.focus.clone().map(Action::Agent);
    if !app.list.open {
        full(app);
    }
}
/// Left at the composer's line start: a helper's parent in the same view; else one view out,
/// the session alone to the list beside it, the composer keeping the keys, and that to the
/// overview, where the list cannot sit beside the session below 80 columns at once.
pub(super) fn outward(app: &mut App) {
    if let Some(parent) = app.row().and_then(|a| a.parent.clone()) {
        keys::focus(app, Some(parent));
        if !app.list.open {
            full(app);
        }
    } else if app.list.open || app.columns < 80 {
        overview(app);
    } else {
        toggle(app);
        app.list.keys = false;
    }
}
/// Shows the addressed session as the main one, as closing the list does.
pub(super) fn full(app: &mut App) {
    if app.sending.is_some() || super::audio::unsent(app) {
        return;
    }
    if let Some(id) = app.focus.clone().filter(|id| app.root.as_ref() != Some(id)) {
        let old = app.root.replace(id.clone());
        if let Some(task) = old.and_then(|old| app.streams.remove(&old)) {
            task.abort();
        }
        app.open(id);
    }
}
/// Clicks share the renderer's actions; a list click addresses the right-hand composer.
pub(super) fn press(app: &mut App, action: Action) {
    match action {
        // A heading's fold toggles.
        Action::Section(id) => _ = app.list.toggled.remove(&id) || app.list.toggled.insert(id),
        Action::New => super::start::open(app),
        Action::Thread(home) => thread(app, &home),
        Action::Agent(id) => {
            app.list.selected = Some(Action::Agent(id.clone()));
            keys::focus(app, Some(id.clone()));
            // From the overview, a refused switch keeps it; an opened agent lands in the list.
            let opened = app.focus == Some(id);
            app.list.keys = app.list.overview.is_some() && !opened;
            if opened && app.list.overview.take().is_some() {
                (app.list.open, app.list.reveal) = (true, true);
                overview::fold(app);
            }
        }
        _ => {}
    }
}
/// List movement moves only the highlight; opening is always an explicit act. Each key arrives
/// as its action's first default key.
pub(super) fn key(app: &mut App, k: KeyEvent) {
    let (action, shown) = (app.list.selected.clone(), app.list.overview.is_some());
    match k.code {
        K::F(2) => toggle(app),
        K::Esc if shown => overview(app),
        K::Esc => app.ask_open = None,
        K::Up | K::Down => {
            let rows = &app.list.rows;
            let at = rows.iter().position(|a| Some(a) == action.as_ref());
            let step = [-1, 1][usize::from(k.code == K::Down)];
            let at = at.unwrap_or(0).saturating_add_signed(step);
            app.list.selected = rows.get(at.min(rows.len().saturating_sub(1))).cloned();
            app.list.reveal = true;
        }
        // The overview is the outermost view; Right there opens an agent, or unfolds a folded
        // group, as Enter does.
        K::Left if shown => {}
        K::Left => app.list.selected = action.as_ref().and_then(|a| parent(app, a)).or(action),
        K::Right | K::Enter if shown => action.into_iter().for_each(|a| press(app, a)),
        K::Right => {
            let id = match &action {
                Some(Action::Section(id)) => id.clone(),
                Some(Action::Agent(id)) => id.to_string(),
                _ => return,
            };
            // Earlier and the servers and residents lines open, the rest unfold.
            match matches!(id.as_str(), "residents" | "servers" | "earlier") {
                true => app.list.toggled.insert(id),
                false => app.list.toggled.remove(&id),
            };
            app.list.selected = action.as_ref().and_then(|a| into(app, a)).or(action);
        }
        K::Char('<' | '>') => widen(app, k.code == K::Char('>')),
        K::Char('n') => match aimed(app) {
            Some(home) => thread(app, &home),
            None => app.say(NOT_RESIDENT),
        },
        K::Char(' ') => match action {
            // In the overview it folds the selected agent's group.
            Some(Action::Agent(id)) if shown => overview::group(app, &id),
            Some(Action::Agent(id)) => press(app, Action::Section(id.to_string())),
            Some(a) => press(app, a),
            None => {}
        },
        K::Enter => match action {
            Some(Action::Agent(id)) => {
                press(app, Action::Agent(id.clone()));
                if app.focus.as_ref() == Some(&id) {
                    toggle(app);
                }
            }
            Some(a) => press(app, a),
            None => {}
        },
        _ => {}
    }
}
/// Widens or narrows the list by four columns, kept for the next start.
pub(super) fn widen(app: &mut App, wider: bool) {
    let step = [-4, 4][usize::from(wider)];
    let now = app.list.area.map_or(32, |a| a.width);
    resize(app, now.saturating_add_signed(step), true);
}
/// F3 or the list's ⤢: shows the overview with the addressed agent selected, or leaves it for
/// the addressed session, unswitched, in the view beneath; with nothing addressed it stays.
pub(super) fn overview(app: &mut App) {
    let shown = app.list.overview.is_some();
    if shown && app.focus.is_none() {
        return;
    }
    (app.list.overview, app.list.keys) = ((!shown).then_some(0), !shown);
    // The overview selects a helper's root, which it lists.
    let id = app
        .focus
        .clone()
        .map(|id| if shown { id } else { root(app, id) });
    app.list.selected = id.map(Action::Agent);
    app.list.reveal = true;
    overview::fold(app);
}
/// Takes `columns` as the list's width from here on, at least 32; `keep` saves it for the next
/// start, once a drag lets go.
pub(super) fn resize(app: &mut App, columns: u16, keep: bool) {
    let columns = columns.max(32);
    app.list.width = Some(columns);
    app.list.dragging = !keep;
    if keep {
        let _ = std::fs::write(&app.list.kept, columns.to_string());
    }
}
fn parent(app: &App, action: &Action) -> Option<Action> {
    let sections = &app.list.layout()?.sections;
    let at = match action {
        Action::Agent(id) => {
            let Some((at, _)) = sections
                .iter()
                .enumerate()
                .filter(|(_, s)| s.sessions.iter().any(|r| &r.session == id))
                .max_by_key(|(_, s)| (s.kind != SectionKind::NeedsYou, s.depth))
            else {
                return app.agents.get(id)?.parent.clone().map(Action::Agent);
            };
            let rows = &sections[at].sessions;
            let row = rows.iter().position(|r| &r.session == id)?;
            if let Some(parent) = rows[..row].iter().rev().find(|r| r.depth < rows[row].depth) {
                return Some(Action::Agent(parent.session.clone()));
            }
            return sections[..=at]
                .iter()
                .rev()
                .find(|s| app.list.rows.contains(&Action::Section(s.id.clone())))
                .map(|s| Action::Section(s.id.clone()));
        }
        Action::Section(id) => sections.iter().position(|s| &s.id == id)?,
        Action::Thread(home) => return Some(Action::Agent(home.clone())),
        _ => return None,
    };
    sections[..at]
        .iter()
        .rev()
        .find(|s| s.depth < sections[at].depth)
        .map(|s| Action::Section(s.id.clone()))
}
fn into(app: &App, action: &Action) -> Option<Action> {
    let sections = &app.list.layout()?.sections;
    match action {
        Action::Section(id) => {
            let at = sections
                .iter()
                .position(|s| &s.id == id && s.kind != SectionKind::Residents)?;
            if let Some(child) = sections
                .get(at + 1)
                .filter(|s| s.depth > sections[at].depth)
            {
                let action = Action::Section(child.id.clone());
                if app.list.rows.contains(&action) || !sections[at].sessions.is_empty() {
                    return Some(action);
                }
                return child
                    .sessions
                    .first()
                    .map(|r| Action::Agent(r.session.clone()))
                    .or(Some(action));
            }
            sections[at]
                .sessions
                .first()
                .map(|r| Action::Agent(r.session.clone()))
        }
        Action::Agent(id) => sections.iter().find_map(|s| {
            let at = s.sessions.iter().position(|r| &r.session == id)?;
            s.sessions
                .get(at + 1)
                .filter(|r| r.depth > s.sessions[at].depth)
                .map(|r| Action::Agent(r.session.clone()))
        }),
        _ => None,
    }
}
/// Opens a top-level agent by a prefix action's name: the next or previous agent in the addressed
/// one's project or project in the list, or the numbered one; the ends stop.
pub(super) fn jump(app: &mut App, action: &str) {
    let here = app.focus.clone().map(|id| root(app, id));
    let projects = projects(app);
    let at = (projects.iter()).position(|p| here.as_ref().is_some_and(|h| p.contains(h)));
    let mine = at.map_or(&[][..], |at| &projects[at]);
    let me = mine.iter().position(|id| Some(id) == here.as_ref());
    let near = |at: Option<usize>, way| at?.checked_add_signed(if way == "next" { 1 } else { -1 });
    let to = match action.rsplit_once('_') {
        Some((way, "project")) => near(at, way).and_then(|i| projects.get(i)?.first()),
        Some((way, "agent")) => near(me, way).and_then(|i| mine.get(i)),
        Some((list, n)) => n.parse::<usize>().ok().and_then(|n| match list {
            "project" => projects.get(n.checked_sub(1)?)?.first(),
            _ => mine.get(n.checked_sub(1)?),
        }),
        None => None,
    };
    if let Some(id) = to.cloned() {
        open(app, id);
    }
}
/// Addresses `id` through the list's switch, as a click on its row does, opening the list; the
/// addressed session is left as it is.
pub(super) fn open(app: &mut App, id: SessionId) {
    if app.focus.as_ref() == Some(&id) {
        return;
    }
    if !app.list.open {
        toggle(app);
    }
    press(app, Action::Agent(id));
}
// The list's top-level agents in its order, whatever heading holds them, by project in the order
// each first shows; less NEEDS YOU, which repeats rows listed below it, and Earlier, whose closed
// roots stay closed.
fn projects(app: &App) -> Vec<Vec<SessionId>> {
    let mut projects: Vec<(&str, Vec<SessionId>)> = Vec::new();
    let sections = (app.list.layout().into_iter()).flat_map(|l| &l.sections);
    let open = sections.filter(|s| !matches!(s.kind, SectionKind::NeedsYou | SectionKind::Earlier));
    let roots = open.flat_map(|s| &s.sessions).filter(|r| r.depth == 0);
    for (row, a) in roots.filter_map(|r| Some((r, app.agents.get(&r.session)?))) {
        match projects.iter_mut().find(|(p, _)| *p == a.project) {
            Some((_, roots)) => roots.push(row.session.clone()),
            None => projects.push((&a.project, vec![row.session.clone()])),
        }
    }
    projects.into_iter().map(|(_, roots)| roots).collect()
}
const NOT_RESIDENT: &str =
    "Select a resident or one of its threads, or name one: /thread <resident>.";
/// The resident home sessions on every server, as the list names them.
fn homes(app: &App) -> impl Iterator<Item = SessionId> {
    (app.servers.iter()).flat_map(|s| s.residents.iter().map(|r| r.session.on(&s.name)))
}
/// The resident whose thread `app` would start: that of the selected row while the list has the
/// keys, else of the addressed session; a thread's helper counts for its thread's resident.
fn aimed(app: &App) -> Option<SessionId> {
    let id = match app.list.selected.as_ref().filter(|_| app.list.keys) {
        Some(Action::Agent(id) | Action::Thread(id)) => id,
        _ => app.focus.as_ref()?,
    };
    let top = root(app, id.clone());
    let server = top.address().map_or("", |(_, server)| server);
    let home =
        (app.agents.get(&top)?.resident.as_ref()).map_or_else(|| top.clone(), |r| r.on(server));
    homes(app).find(|h| *h == home)
}
/// Addresses the most recent top-level agent that is neither a resident's home nor its thread,
/// the list keeping the keys: `razor --agents` with nothing else to open (Trey, 2026-11-03).
pub(super) fn recent(app: &mut App) {
    let id = (app.agents.values())
        .filter(|a| a.parent.is_none() && a.resident.is_none())
        .filter(|a| !homes(app).any(|h| h == a.session))
        .max_by_key(|a| (a.last_activity, &a.session))
        .map(|a| a.session.clone());
    if let Some(id) = id {
        press(app, Action::Agent(id));
        app.list.keys = true;
    }
}
/// Aims the composer at a fresh thread with the resident whose home session is `home`.
fn thread(app: &mut App, home: &SessionId) {
    (app.list.keys, app.list.overview) = (false, None);
    let (name, server) = home.address().unwrap_or((home.clone(), ""));
    super::start::aim(app, server.into(), Default::default(), Some(name));
}
fn root(app: &App, mut id: SessionId) -> SessionId {
    for _ in 0..app.agents.len() {
        let Some(Action::Agent(parent)) = parent(app, &Action::Agent(id.clone())) else {
            break;
        };
        id = parent;
    }
    id
}
/// The agents list's own commands, `/group`, `/agents`, `/new`, `/thread`, `/view` and `/views`;
/// false for another.
pub(super) fn command(app: &mut App, word: &str, args: &str) -> bool {
    match word {
        "/group" => groups::file(app, args),
        "/agents" => toggle(app),
        "/new" => super::start::open(app),
        "/thread" => {
            let named = homes(app).find(|h| {
                h.as_str() == args || h.address().is_some_and(|(n, _)| n.as_str() == args)
            });
            match named.or_else(|| aimed(app).filter(|_| args.is_empty())) {
                Some(home) => thread(app, &home),
                None => app.say(NOT_RESIDENT),
            }
        }
        "/view"
            if (app.list.status.as_ref()).is_some_and(|s| s.names.iter().any(|n| n == args)) =>
        {
            app.list.view = Some(args.into())
        }
        "/view" => app.say("That view is unknown; use a name from /views."),
        "/views" => app.editing = Some(super::editor::Editing::Views),
        _ => return false,
    }
    true
}
/// Slides the shared list beside the cockpit; narrow screens give the whole width to its owner.
pub(super) fn frame(
    app: &mut App,
    b: &mut Buffer,
    area: Rect,
    question: Question,
    now: u64,
    hits: &mut Vec<Hit>,
) -> Option<(u16, u16)> {
    let elapsed = super::motion::played(super::motion::on(app), now, app.list.changed, 250);
    let share = super::motion::ease_out(elapsed);
    let share = if app.list.open { share } else { 1. - share };
    if elapsed < 1. {
        app.next.every(now, 33);
    }
    let wide = area.width >= 80;
    let width = if wide {
        let chosen = app.list.width.unwrap_or((area.width / 3).clamp(32, 64));
        (chosen.clamp(32, area.width.saturating_sub(40).max(32)) as f64 * share).round() as u16
    } else {
        area.width * u16::from(app.list.open && app.list.keys)
    };
    let list = Rect { width, ..area };
    app.list.area = (width > 0).then_some(list);
    let right = Rect::new(area.x + width, area.y, area.width - width, area.height);
    // Beside the list the pane is closed until asked for.
    let pane = app.pane_open.replace(app.pane_open == Some(true));
    let cursor = (right.width > 0).then(|| view::normal(app, b, right, question, now, hits));
    app.pane_open = pane;
    // Narrower than its edges and a column each side, the sliding list is blank.
    if width > 5 && area.height > 2 {
        draw(app, b, list, wide, now, hits);
    }
    cursor.flatten().filter(|_| !app.list.keys)
}

// The list in its frame, its drag edge when wide, and its rows from the scroll, which reveals a
// moved selection; the rows' actions are the arrows' stops, and the bottom edge names the
// addressed row while it is scrolled away.
fn draw(app: &mut App, b: &mut Buffer, list: Rect, wide: bool, now: u64, hits: &mut Vec<Hit>) {
    let title = format!("AGENTS · {}", app.list.view.as_deref().unwrap_or("…"));
    let edge = if app.list.dragging { WHITE } else { CYAN };
    let inner = super::panel::frame_colour(b, list, &title, edge);
    // At the top right, ‹ closes the list and ⤢ opens the overview, each with its key.
    let mut x = list.right().saturating_sub(2);
    let marks = [("⤢", "global.overview"), ("‹", "list.open")];
    for ((mark, key), action) in marks.into_iter().zip([Action::Overview, Action::Close]) {
        let words = format!(" {mark} {} ", super::bindings::hint(app, key));
        let w = width(&words) as u16;
        x = x.saturating_sub(w);
        put(b, x, list.y, &words, w, style(GRAY, GLASS));
        hits.push((Rect::new(x, list.y, w, 1), action));
    }
    if wide {
        let column = Rect::new(list.right() - 1, inner.y, 1, inner.height);
        hits.push((column, Action::Edge));
    }
    let rows = rows(app, inner.width, now);
    // A parent's own fold is no stop of its own: Space on its row folds it.
    let own = |r: &status::Row, a: &Action| matches!((&r.1, a), (Some(Action::Agent(id)), Action::Section(s)) if s == id.as_str());
    let (mut seen, actions) = (
        BTreeSet::new(),
        rows.iter()
            .flat_map(|r| (r.2.iter().map(|(_, a)| a).filter(|a| !own(r, a))).chain(r.1.iter())),
    );
    // An agent listed twice, a shortcut and its row, is one stop for the arrows.
    let unique = actions.filter(|a| !matches!(a, Action::Agent(id) if !seen.insert(id)));
    app.list.rows = unique.cloned().collect();
    // A folded heading's mark and its line are one stop.
    app.list.rows.dedup();
    let selected = rows
        .iter()
        .position(|status::Row(_, a, _)| a.is_some() && a == &app.list.selected);
    let height = inner.height as usize;
    // Under the overview, the reveal is the overview's.
    if app.list.overview.is_none()
        && std::mem::take(&mut app.list.reveal)
        && let Some(at) = selected
    {
        app.list.scroll = app.list.scroll.min(at).max(at.saturating_sub(height - 1));
    }
    let scroll = app.list.scroll.min(rows.len().saturating_sub(height));
    // The bottom edge names the addressed row while it is scrolled away.
    let here = rows.iter().position(|row| row.0.style.bg == Some(YOU_BG));
    let name = app.focus.as_ref().and_then(|id| app.agents.get(id));
    let name = name.map_or("", |a| a.name.as_str());
    let (words, ink) = match here {
        Some(at) if at < scroll => (format!(" ▲ talking to {name} "), bold(CYAN, GLASS)),
        Some(at) if at >= scroll + height => (format!(" ▼ talking to {name} "), bold(CYAN, GLASS)),
        _ => (String::new(), style(GRAY, GLASS)),
    };
    put(b, inner.x, inner.bottom(), &words, inner.width, ink);
    app.list.scroll = scroll;
    for (y, status::Row(words, action, fold)) in
        (inner.y..inner.bottom()).zip(rows.into_iter().skip(scroll))
    {
        let r = Rect::new(inner.x, y, inner.width, 1);
        if words.style.bg == Some(YOU_BG) {
            fill(
                b,
                Rect::new(list.x + 1, y, list.width - 2, 1),
                style(WHITE, YOU_BG),
            );
            put(b, list.x + 1, y, "▌", 1, style(CYAN, YOU_BG));
        }
        line(b, r, y, &words, style(WHITE, GLASS));
        let fold = fold
            .into_iter()
            .map(|(x, a)| (Rect::new(r.x + x, y, 2, 1), a));
        hits.extend(action.map(|a| (r, a)).into_iter().chain(fold));
    }
}
