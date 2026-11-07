//! The agents view: the list beside the cockpit and the full-screen overview, their keys and
//! commands; `sections` draws the list's rows from the server-resolved layout, and `overview`
//! draws the same layout over the whole screen.
mod groups;
mod overview;
mod panel;
mod sections;
mod servers;
mod tiles;
use super::view::{self, Action, Hit};
use super::{App, keys, text::*};
use crate::protocol::views::{Layout, Section, SectionKind, ViewsStatus};
use crate::protocol::{SessionId, now};
pub(super) use groups::edit;
pub(super) use overview::{covers, draw as overlay};
use panel::on;
use ratatui::{buffer::Buffer, layout::Rect};
use sections::{Row, rows};
use std::collections::BTreeSet;
/// The list's hint, a pair each, those with an unbound action left out; of actions split by `|`, the
/// first bound one's keys.
const LIST_KEYS: [&str; 6] = [
    "<list.beside|list.accept> open",
    "<composer.switch> back",
    "<list.overview|global.overview> overview",
    "<list.previous_agent|list.up> <list.next_agent|list.down> move",
    "<list.fold> fold",
    "<list.group_1>-<list.group_9> group",
];
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
    (app.list.open, app.list.overview, app.list.keys) = (!app.list.open, None, false);
    app.list.changed = now();
    match app.list.open {
        true => choose(app),
        false => full(app),
    }
}
/// Left at the composer's line start: a helper's parent in the same view; else `line_start.choose`
/// gives the list the keys, and `line_start.outward` goes one view out, the session alone to the
/// list beside it, the composer keeping the keys, and that to the overview, where the list cannot
/// sit beside the session below 80 columns at once.
pub(super) fn outward(app: &mut App, action: &str) {
    if let Some(parent) = app.row().and_then(|a| a.parent.clone()) {
        keys::focus(app, Some(parent));
        app.after(|app| {
            if !app.list.open {
                full(app);
            }
        });
    } else if action == "line_start.choose" {
        choose(app);
    } else if app.list.open || app.columns < 80 {
        overview(app);
    } else {
        toggle(app);
        app.list.keys = false;
    }
}
/// Gives the list the keys with the addressed agent selected, opening the list if it is closed;
/// a new agent's or thread's composer keeps the row that aimed it.
pub(super) fn choose(app: &mut App) {
    if !app.list.open {
        return toggle(app);
    }
    (app.list.keys, app.list.reveal) = (true, true);
    let row = app.focus.clone().map(Action::Agent);
    app.list.selected = row.or(app.list.selected.take());
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
        app.rooted(id);
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
            // From the overview, a refused or failed switch keeps it; an opened agent lands in
            // the list.
            app.list.keys = app.list.overview.is_some();
            keys::focus(app, Some(id.clone()));
            app.after(|app| {
                if app.focus == Some(id) && app.list.overview.take().is_some() {
                    (app.list.keys, app.list.open, app.list.reveal) = (false, true, true);
                    overview::fold(app);
                }
            });
        }
        _ => {}
    }
}
/// List movement moves only the highlight; opening is always an explicit act. The defaults' actions
/// step every row, open an agent closing the list, and move out to a row's parent or into a
/// heading; the others pass over helpers, open beside the list and open the overview.
pub(super) fn key(app: &mut App, action: &str) {
    let (selected, shown) = (app.list.selected.clone(), app.list.overview.is_some());
    match action.trim_start_matches("list.") {
        "open" => toggle(app),
        "close" if shown => overview(app),
        "close" => app.ask_open = None,
        "up" | "down" => step(app, action == "list.down", true),
        "previous_agent" | "next_agent" => step(app, action == "list.next_agent", false),
        // The overview is the outermost view.
        "left" | "overview" if shown => {}
        "left" => app.list.selected = selected.as_ref().and_then(|a| parent(app, a)).or(selected),
        "overview" => overview(app),
        // In the overview Right and Enter open an agent, or unfold a folded group.
        "right" | "accept" | "beside" if shown => selected.into_iter().for_each(|a| press(app, a)),
        // `right` moves in: to a heading's first row, or to an agent's first helper.
        "right" => {
            let id = match &selected {
                Some(Action::Section(id)) => id.clone(),
                Some(Action::Agent(id)) => id.to_string(),
                _ => return,
            };
            // Earlier and the servers and residents lines open, the rest unfold.
            match matches!(id.as_str(), "residents" | "servers" | "earlier") {
                true => app.list.toggled.insert(id),
                false => app.list.toggled.remove(&id),
            };
            app.list.selected = selected.as_ref().and_then(|a| into(app, a)).or(selected);
        }
        // An agent opened by `accept` takes the whole screen; `beside` keeps the list.
        "accept" if let Some(Action::Agent(id)) = selected.clone() => {
            press(app, Action::Agent(id.clone()));
            app.after(move |app| {
                if app.focus == Some(id) {
                    toggle(app);
                }
            });
        }
        "accept" | "beside" => selected.into_iter().for_each(|a| press(app, a)),
        "wider" | "narrower" => widen(app, action == "list.wider"),
        "thread" => match aimed(app) {
            Some(home) => thread(app, &home),
            None => app.say(NOT_RESIDENT),
        },
        "fold" => match selected {
            // In the overview it folds the selected agent's group.
            Some(Action::Agent(id)) if shown => overview::group(app, &id),
            Some(Action::Agent(id)) => press(app, Action::Section(id.to_string())),
            Some(a) => press(app, a),
            None => {}
        },
        // The catalogue's groups run from group_1 to group_9.
        g if g.starts_with("group_") => numbered(app, usize::from(g.as_bytes()[6] - b'1')),
        _ => {}
    }
}
/// Moves the selection a row from where it is, which may be a helper: to the next row of any kind,
/// or of the stops that pass over helpers.
fn step(app: &mut App, down: bool, every: bool) {
    let here = |a: &Action| Some(a) == app.list.selected.as_ref();
    let rows = (app.list.rows.iter()).filter(|a| every || here(a) || stop(app, a));
    let stops: Vec<_> = rows.cloned().collect();
    let (at, step) = (stops.iter().position(here), [-1, 1][usize::from(down)]);
    let at = at.unwrap_or(0).saturating_add_signed(step);
    app.list.selected = stops.get(at.min(stops.len().saturating_sub(1))).cloned();
    app.list.reveal = true;
}
/// Selects the `n`th top-level group's first agent, or the group while it is folded or the
/// overview is shown, which moves an open group's selection to its first tile.
fn numbered(app: &mut App, n: usize) {
    let Some(tree) = overview::groups(app).get(n).copied() else {
        return;
    };
    let first = panel::rows(tree).find(|r| r.depth == 0);
    let first = first.map(|r| Action::Agent(r.session.clone()));
    let open = !app.list.toggled.contains(&tree[0].id) && app.list.overview.is_none();
    let section = Action::Section(tree[0].id.clone());
    app.list.selected = first.filter(|_| open).or(Some(section));
    app.list.reveal = true;
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
/// Whether the arrows stop on a row: on anything but a helper or a helper's finished line, and
/// on a helper listed as needing the person.
fn stop(app: &App, row: &Action) -> bool {
    let sections = (app.list.layout().into_iter()).flat_map(|l| &l.sections);
    let mut needs = sections.filter(|s| s.kind == SectionKind::NeedsYou);
    match row {
        Action::Agent(id) if app.agents.get(id).is_some_and(|a| a.parent.is_some()) => {
            needs.any(|s| s.sessions.iter().any(|r| &r.session == id))
        }
        Action::Section(id) => !id.starts_with("finished/"),
        _ => true,
    }
}
/// The row `left` moves out to: an agent's parent row, else the nearest listed heading over it, or
/// its recorded parent while it is in no section; a thread's resident; a heading's outer heading.
fn parent(app: &App, action: &Action) -> Option<Action> {
    let sections = &app.list.layout()?.sections;
    let heading = |s: &Section| Action::Section(s.id.clone());
    let at = match action {
        Action::Agent(id) => {
            let holds = |s: &&Section| s.sessions.iter().any(|r| &r.session == id);
            let mine = (sections.iter().enumerate()).filter(|(_, s)| holds(s));
            let Some((at, _)) =
                mine.max_by_key(|(_, s)| (s.kind != SectionKind::NeedsYou, s.depth))
            else {
                return app.agents.get(id)?.parent.clone().map(Action::Agent);
            };
            let rows = &sections[at].sessions;
            let row = rows.iter().position(|r| &r.session == id)?;
            if let Some(up) = rows[..row].iter().rev().find(|r| r.depth < rows[row].depth) {
                return Some(Action::Agent(up.session.clone()));
            }
            let listed = |s: &&Section| app.list.rows.contains(&heading(s));
            return sections[..=at].iter().rev().find(listed).map(heading);
        }
        Action::Section(id) => sections.iter().position(|s| &s.id == id)?,
        Action::Thread(home) => return Some(Action::Agent(home.clone())),
        _ => return None,
    };
    let outer = (sections[..at].iter().rev()).find(|s| s.depth < sections[at].depth);
    outer.map(heading)
}
/// The row `right` moves into: a heading's first listed heading or agent, an agent's first helper.
fn into(app: &App, action: &Action) -> Option<Action> {
    let sections = &app.list.layout()?.sections;
    let first = |s: &Section| s.sessions.first().map(|r| Action::Agent(r.session.clone()));
    match action {
        Action::Section(id) => {
            let mut at = sections.iter();
            let at = at.position(|s| &s.id == id && s.kind != SectionKind::Residents)?;
            let Some(child) = (sections.get(at + 1)).filter(|s| s.depth > sections[at].depth)
            else {
                return first(&sections[at]);
            };
            let heading = Action::Section(child.id.clone());
            match app.list.rows.contains(&heading) || !sections[at].sessions.is_empty() {
                true => Some(heading),
                false => first(child).or(Some(heading)),
            }
        }
        Action::Agent(id) => sections.iter().find_map(|s| {
            let at = s.sessions.iter().position(|r| &r.session == id)?;
            let next = (s.sessions.get(at + 1)).filter(|r| r.depth > s.sessions[at].depth);
            next.map(|r| Action::Agent(r.session.clone()))
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
// each first shows; of NEEDS YOU only the rows listed nowhere else, and nothing of Earlier, whose
// closed roots stay closed.
fn projects(app: &App) -> Vec<Vec<SessionId>> {
    let mut projects: Vec<(&str, Vec<SessionId>)> = Vec::new();
    use SectionKind::{Earlier, NeedsYou};
    let alone = overview::alone(app);
    let sections = (app.list.layout().into_iter()).flat_map(|l| &l.sections);
    let open = sections.filter(|s| s.kind != Earlier);
    let open = open.filter_map(|s| [Some(s), alone.as_ref()][usize::from(s.kind == NeedsYou)]);
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
/// Addresses the top-level agent, neither a resident's home nor its thread, the person last wrote
/// to, the list keeping the keys: `razor --agents` with nothing else to open (Trey, 2026-11-03).
pub(super) fn recent(app: &mut App) {
    let id = (app.agents.values())
        .filter(|a| a.parent.is_none() && a.resident.is_none())
        .filter(|a| !homes(app).any(|h| h == a.session))
        .filter_map(|a| Some((a.person_input_at?, &a.session)))
        .max()
        .map(|(_, id)| id.clone());
    if let Some(id) = id {
        press(app, Action::Agent(id));
        app.list.keys = true;
    }
}
/// Unfolds what hides `id`'s rows: each section holding it and those over it, the rows over it
/// there, and the finished lines it or they fold into. Earlier, Residents and a finished line open
/// when toggled; the other folds shut.
pub(super) fn unfold(app: &mut App, id: &SessionId) {
    let (mut over, mut folds) = (Vec::new(), Vec::new());
    for s in app.list.layout().iter().flat_map(|l| &l.sections) {
        over.truncate(s.depth.into());
        let opens = matches!(s.kind, SectionKind::Earlier | SectionKind::Residents);
        over.push((s.id.clone(), opens));
        let Some(at) = s.sessions.iter().position(|r| &r.session == id) else {
            continue;
        };
        folds.extend(over.iter().cloned());
        let mut depth = u16::MAX;
        for (i, r) in s.sessions[..=at].iter().enumerate().rev() {
            if r.depth < depth {
                depth = r.depth;
                folds.extend((i < at).then(|| (r.session.to_string(), false)));
                let finished = app.agents.get(&r.session).filter(|_| r.folded);
                folds.extend(finished.map(|a| (sections::finished(a), true)));
            }
        }
    }
    for (fold, opens) in folds {
        match opens {
            true => app.list.toggled.insert(fold),
            false => app.list.toggled.remove(&fold),
        };
    }
    app.list.reveal = true;
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
    let cursor = (right.width > 0).then(|| view::normal(app, b, right, now, hits));
    app.pane_open = pane;
    // Narrower than its edges and a column each side, the sliding list is blank.
    if width > 5 && area.height > 2 {
        draw(app, b, list, wide, now, hits);
    }
    cursor.flatten().filter(|_| !app.list.keys)
}

// The list on the panel's ground: its header, its drag edge when wide, its rows from the scroll,
// which reveals a moved selection, and its keys on the last row, or the addressed row named while
// it is scrolled away. The rows' actions are the arrows' stops.
fn draw(app: &mut App, b: &mut Buffer, list: Rect, wide: bool, now: u64, hits: &mut Vec<Hit>) {
    fill(b, list, style(DATA, PANEL));
    header(app, b, list, now, hits);
    let inner = Rect::new(
        list.x,
        list.y + 4,
        list.width,
        list.height.saturating_sub(5),
    );
    let rows = rows(app, inner.width, now);
    // An agent listed twice, a shortcut and its row, is one stop for the arrows, as are the
    // lines of one row; a fold mark is none, since Space on its row folds it.
    let mut seen = BTreeSet::new();
    let actions = rows.iter().filter_map(|r| r.1.as_ref());
    let unique = actions.filter(|a| !matches!(a, Action::Agent(id) if !seen.insert(id)));
    app.list.rows = unique.cloned().collect();
    app.list.rows.dedup();
    let selected = rows
        .iter()
        .position(|Row(_, a, _)| a.is_some() && a == &app.list.selected);
    let height = inner.height as usize;
    // Under the overview, the reveal is the overview's.
    if app.list.overview.is_none()
        && std::mem::take(&mut app.list.reveal)
        && let Some(at) = selected
    {
        app.list.scroll = app.list.scroll.min(at).max(at.saturating_sub(height - 1));
    }
    let scroll = app.list.scroll.min(rows.len().saturating_sub(height));
    let here = rows.iter().position(|row| sections::here(&row.0));
    let away = here.filter(|at| *at < scroll || *at >= scroll + height);
    let foot = Rect::new(list.x, inner.bottom(), list.width, 1);
    keys(app, b, foot, away.map(|at| at < scroll));
    app.list.scroll = scroll;
    for (y, Row(words, action, fold)) in
        (inner.y..inner.bottom()).zip(rows.into_iter().skip(scroll))
    {
        let r = Rect::new(inner.x, y, inner.width, 1);
        line(b, r, y, &words, style(WHITE, PANEL));
        let fold = fold.map(|a| (Rect::new(r.x + 1, y, 2, 1), a));
        hits.extend(action.map(|a| (r, a)).into_iter().chain(fold));
    }
    // The drag edge, over the rows' right column, takes the click.
    if wide {
        let column = Rect::new(list.right() - 1, inner.y, 1, inner.height);
        hits.push((column, Action::Edge));
    }
    // Rows drawn on the glass, the servers line's, take the panel's ground.
    for at in inner.positions() {
        if b[at].bg == GLASS {
            b[at].bg = PANEL;
        }
    }
}

// The list's last row in `r`: the addressed agent's name while its row is scrolled away, `above`
// or below; else the keys of whichever actions are bound, those after the first whole or not at
// all: while the composer has the keys, how to give them to the list, else the list's.
fn keys(app: &App, b: &mut Buffer, r: Rect, above: Option<bool>) {
    let (words, ink) = match above {
        Some(above) => {
            let name = app.focus.as_ref().and_then(|id| app.agents.get(id));
            let name = name.map_or("", |a| a.name.as_str());
            let mark = ["▼", "▲"][usize::from(above)];
            (format!(" {mark} talking to {name}"), bold(CYAN, PANEL))
        }
        None => {
            let bound = |t: &&str| super::bindings::bound(app, t);
            let into = ["<composer.switch>", "<line_start.choose>"];
            let into: Vec<_> = into.into_iter().filter(bound).collect();
            let keys = match app.list.keys || into.is_empty() {
                true => LIST_KEYS.join("  "),
                false => format!("{} to choose an agent", into.join(" or ")),
            };
            let keys = super::bindings::hints(app, &keys, "  ");
            let mut pairs = keys.split("  ");
            let mut words = format!(" {}", pairs.next().unwrap_or_default());
            for pair in pairs {
                let more = format!("{words}  {pair}");
                if width(&more) >= r.width as usize {
                    break;
                }
                words = more;
            }
            (words, style(GRAY, PANEL))
        }
    };
    put(b, r.x, r.y, &clip(&words, r.width as usize), r.width, ink);
}

// The list's header, four rows framed in the panel's ink: the tabs, ‹ and ⤢ with their keys at
// the right of its top edge; the Fine clock, the date, the seconds, the needs-you tag, the census
// and its bar, the tag's words and the bar dropped where the width runs out.
fn header(app: &App, b: &mut Buffer, list: Rect, now: u64, hits: &mut Vec<Hit>) {
    let (y, right) = (list.y, list.right().saturating_sub(1));
    let view = app.list.view.as_deref().unwrap_or("…").to_uppercase();
    let agents = format!(" AGENTS · {view} ");
    let top = Rect::new(list.x, list.y, list.width, 4);
    tiles::frame(b, top, (PANEL_INK, PANEL), true, None);
    let tabs = [
        (" RAZOR ", CHIP_INK, CYAN),
        (agents.as_str(), CYAN, CLOCK_BG),
    ];
    panel::write(b, list.x + 2, y, &panel::tabs(&tabs, PANEL));
    // At the top right, ‹ closes the list and ⤢ opens the overview, each with its key.
    let mut x = right.saturating_sub(1);
    let marks = [("⤢", "global.overview"), ("‹", "list.open")];
    for ((mark, key), action) in marks.into_iter().zip([Action::Overview, Action::Close]) {
        let words = format!(" {mark} {} ", super::bindings::hint(app, key));
        x = x.saturating_sub(width(&words) as u16);
        let words = on(words, mix(PANEL_INK, WHITE, 0.35), PANEL, false);
        let end = panel::write(b, x, y, &[words]);
        hits.push((Rect::new(x, y, end - x, 1), action));
    }
    let x = panel::clock(app, b, list.x + 3, y + 1, now) + 1;
    let day = zoned(now, app.zone.as_ref()).map(|t| t.strftime("%a %d %b").to_string());
    let day = day.unwrap_or_default().to_uppercase();
    let alone = overview::alone(app);
    let roots = overview::panels(app, &alone)
        .into_iter()
        .flat_map(panel::rows);
    let roots = roots.filter(|r| r.depth == 0);
    let census = panel::census(roots.filter_map(|r| app.agents.get(&r.session)));
    let needs = census[0] + census[1];
    let tags = [format!(" ◆ {needs} NEED YOU "), format!(" ◆ {needs} ")];
    let room = right.saturating_sub(x + 3 + width(&day) as u16) as usize;
    let tag = tags
        .into_iter()
        .find(|t| width(t) <= room)
        .filter(|_| needs > 0);
    let tag = on(tag.unwrap_or_default(), CHIP_INK, AMBER, true);
    let at = right.saturating_sub(1 + width(&tag.content) as u16);
    panel::write(b, at, y + 1, &[tag]);
    let day = on(clip(&day, room + 2), WHITE, PANEL, true);
    panel::write(b, x + 1, y + 1, &[day]);
    let seconds = panel::seconds(app, now).unwrap_or_default();
    panel::write(b, x + 1, y + 2, &[on(seconds, MAGENTA, PANEL, true)]);
    let counts = panel::counts(census, PANEL);
    if x + 4 + (spans_width(&counts) as u16) < right {
        let x = panel::write(b, x + 4, y + 2, &counts) + 1;
        let cells = right.saturating_sub(x + 1) as usize;
        if cells > 6 {
            panel::write(b, x, y + 2, &panel::bar(census, cells, PANEL));
        }
    }
}
