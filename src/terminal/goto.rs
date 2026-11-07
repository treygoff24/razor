//! Go to: every agent the terminal sees, by server and project with each one's descendants
//! nested under it, filtered as the person types, above the past conversations the same words
//! find; Enter opens the chosen one where it lives.
use super::{App, agents, bindings, cards::Card, panel, status, text::*, view::Action};
use crate::protocol::{AgentView, Found, SessionId};
use crossterm::event::{KeyCode as K, KeyEvent};
use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use std::{cmp::Reverse, collections::BTreeMap};
use tokio::time::{Duration, Instant};

/// A selected conversation, distinguished from its agent row when it is a history hit.
type Pick = (SessionId, bool);

#[derive(Default)]
struct Row {
    words: Line<'static>,
    session: Option<SessionId>,
    group: usize,
    matched: bool,
    history: bool,
    place: String,
}
impl Row {
    /// A row naming a group, or saying why it has none, which nothing selects.
    fn head(words: impl Into<Line<'static>>, group: usize) -> Self {
        let words = words.into();
        Row {
            words,
            group,
            ..Row::default()
        }
    }
}

#[derive(Default)]
/// Disposable: a 200-byte query, at most 50 history hits and the status stream’s bounded rows.
pub struct Goto {
    query: String,
    /// The search line has the keys: plain characters type into it.
    pub typing: bool,
    selected: Option<Pick>,
    // The last draw's highlightable rows in order, each with its group.
    picks: Vec<(Pick, usize)>,
    scroll: usize,
    /// When the history search starts: 250 ms after the words last changed.
    pub due: Option<Instant>,
    // The newest history answer and the words it answers.
    history: Option<(String, Result<Found, super::Error>)>,
}

/// Opens Go to with the addressed session highlighted.
pub fn open(app: &mut App) {
    let g = app.goto.insert(Goto::default());
    g.selected = app.focus.clone().map(|id| (id, false));
}

/// Takes every key while open; plain characters arrive as text while the search line types.
pub fn key(app: &mut App, k: KeyEvent, action: &str) {
    let Some(g) = app.goto.as_mut() else { return };
    match (action, k.code) {
        ("", K::Char(c)) if g.typing && g.query.len() + c.len_utf8() <= 200 => {
            edit(g, |q| q.push(c))
        }
        ("goto.erase", _) if g.typing => edit(g, |q| _ = q.pop()),
        ("goto.cancel", _) if g.typing || !g.query.is_empty() => {
            g.typing = false;
            edit(g, String::clear);
        }
        ("goto.cancel", _) => app.goto = None,
        ("goto.search", _) => g.typing = true,
        ("goto.accept", _) => {
            if let Some((id, _)) = g.selected.clone() {
                app.goto = None;
                go(app, id);
            }
        }
        ("goto.up" | "goto.down", _) => step(g, action == "goto.down", false),
        ("goto.left" | "goto.right", _) => step(g, action == "goto.right", true),
        _ => {}
    }
}

// Changes the words: the highlight goes to the first match and the history search waits for a rest.
fn edit(g: &mut Goto, change: impl FnOnce(&mut String)) {
    change(&mut g.query);
    let words = !g.query.trim().is_empty();
    g.selected = None;
    g.due = words.then(|| Instant::now() + Duration::from_millis(250));
    g.history = g.history.take().filter(|_| words);
}

// Moves the highlight a row, or with `group` to the first row of the next or previous group.
fn step(g: &mut Goto, forward: bool, group: bool) {
    let picks = &g.picks;
    let at = picks.iter().position(|p| Some(&p.0) == g.selected.as_ref());
    let to = match (at, group) {
        (None, _) => Some(0),
        (Some(at), false) if forward => Some(at + 1),
        (Some(at), false) => at.checked_sub(1),
        (Some(at), true) => {
            let (here, mut groups) = (picks[at].1, picks.iter().map(|p| p.1));
            let next = match forward {
                true => groups.find(|&n| n > here),
                false => groups.rev().find(|&n| n < here),
            };
            next.and_then(|n| picks.iter().position(|p| p.1 == n))
        }
    };
    if let Some((pick, _)) = to.and_then(|i| picks.get(i)) {
        g.selected = Some(pick.clone());
    }
}

/// Shows `id`'s tree as the main session, as the agents list's Enter does, then addresses `id`
/// as a click on its row does, so a descendant opens in its split.
fn go(app: &mut App, id: SessionId) {
    let mut top = id.clone();
    for _ in 0..app.agents.len() {
        let parent = app.agents.get(&top).and_then(|a| a.parent.as_ref());
        let live = parent.filter(|p| app.agents.get(*p).is_some_and(|a| a.settled.is_none()));
        let Some(parent) = live else {
            break;
        };
        top = parent.clone();
    }
    if app.root.as_ref() != Some(&top) {
        agents::press(app, Action::Agent(top));
        agents::full(app);
    }
    agents::unfold(app, &id);
    agents::press(app, Action::Agent(id));
}

/// Starts the history search once its words have rested.
pub fn tick(app: &mut App) {
    let due = |g: &&mut Goto| g.due.is_some_and(|at| at <= Instant::now());
    let Some(g) = app.goto.as_mut().filter(due) else {
        return;
    };
    if app.pending >= 8 {
        g.due = Some(Instant::now() + Duration::from_millis(250));
        return;
    }
    g.due = None;
    let words = g.query.trim().to_owned();
    let mut url = reqwest::Url::parse("http://localhost/v1/search").expect("static URL");
    url.query_pairs_mut().append_pair("q", &words);
    let path = format!("v1/search?{}", url.query().unwrap_or_default());
    app.spawn(move |api| async move {
        let read = api.json(reqwest::Method::GET, &path, None).await;
        let invalid = "The search answer is invalid; reconnect to a matching server.";
        let found = read.and_then(|v| serde_json::from_value(v).map_err(super::fail(invalid)));
        move |app: &mut App| {
            if let Some(g) = app.goto.as_mut().filter(|g| g.query.trim() == words) {
                g.history = Some((words, found));
            }
        }
    });
}

// Whether every word is a subsequence of the name or begins a word of the headline, the project
// or the card's state word.
fn found(words: &[String], a: &AgentView, state: &str) -> bool {
    let name = a.name.to_lowercase();
    let topic = format!("{} {} {state}", a.headline, a.project).to_lowercase();
    let starts = |i: usize| i == 0 || !topic[..i].ends_with(char::is_alphanumeric);
    words.iter().all(|w| {
        let mut letters = name.chars();
        w.chars().all(|c| letters.any(|n| n == c))
            || topic.match_indices(w.as_str()).any(|(i, _)| starts(i))
    })
}

/// Appends `a`'s subtree as shown, each kept agent's depth, card and whether it matched; an agent
/// is kept when it or a descendant matched. True when any did.
fn tree<'a>(
    (app, words, now): (&App, &[String], u64),
    kids: &BTreeMap<&SessionId, Vec<&'a AgentView>>,
    (a, depth): (&'a AgentView, usize),
    out: &mut Vec<(usize, Card<'a>, bool)>,
) -> bool {
    let card = Card::of(app, a, now);
    let (at, mut any) = (out.len(), found(words, a, card.word));
    out.push((depth, card, any));
    for kid in kids.get(&a.session).into_iter().flatten() {
        any |= tree((app, words, now), kids, (kid, depth + 1), out);
    }
    if !any {
        out.truncate(at);
    }
    any
}

// `s` in `n` columns, cut or padded.
fn cell(s: &str, n: usize) -> String {
    let s = clip(s, n.saturating_sub(1));
    format!("{s}{}", " ".repeat(n.saturating_sub(width(&s))))
}

// A session's name, else its id.
fn name<'a>(name: &'a str, id: &'a SessionId) -> &'a str {
    if name.is_empty() { id.as_str() } else { name }
}

// The same card facts as the agents list, in columns, with the selected row's place.
fn agent_row(
    app: &App,
    (depth, card, hit): (usize, Card<'_>, bool),
    columns: usize,
    group: usize,
) -> Row {
    let (a, ink) = (card.agent, [GRAY, WHITE][usize::from(hit)]);
    let branch = match depth {
        0 => String::new(),
        _ => format!("{}└ ", "│ ".repeat(depth - 1)),
    };
    let title = cell(name(&a.name, &a.session), 28 - width(&branch).min(20));
    let words = Line::from(vec![
        span(format!("    {branch}"), GRAY_DIM),
        span(format!("{} ", card.glyph), card.tone),
        span(title, ink),
        span(cell(&model(&a.model), 12), GRAY),
        span(cell(card.word, 16), card.tone),
        span(a.headline.clone(), [GRAY, DATA][usize::from(hit)]),
    ]);
    let repo = a.repo.as_deref().unwrap_or(&a.project);
    let branch: String = a.branch.iter().map(|b| format!(" · {b}")).collect();
    let n = a.changed_files.len();
    let at = status::location(app, &a.session, columns / 2);
    let place = format!("{at} · {repo}{branch} · {n} changed");
    Row {
        words,
        session: Some(a.session.clone()),
        group,
        matched: hit,
        history: false,
        place,
    }
}

// Every row: servers, their projects and the projects' trees, then the history group.
fn rows(app: &App, g: &Goto, columns: usize, now: u64) -> Vec<Row> {
    let live = |id: &SessionId| app.agents.get(id).is_some_and(|a| a.settled.is_none());
    let (mut kids, mut roots) = (BTreeMap::<_, Vec<_>>::new(), Vec::new());
    for a in app.agents.values().filter(|a| live(&a.session)) {
        match a.parent.as_ref().filter(|p| live(p)) {
            Some(parent) => kids.entry(parent).or_default().push(a),
            None => roots.push(a),
        }
    }
    for k in kids.values_mut() {
        k.sort_by_key(|a| a.launched);
    }
    // This server first, then the others in the status order; projects and roots newest first.
    let server = |a: &AgentView| {
        let name = a.session.as_str().split_once('@').map_or("", |(_, s)| s);
        let at = app.servers.iter().position(|s| s.name == name);
        (!name.is_empty(), at)
    };
    let mut newest = BTreeMap::new();
    for a in &roots {
        let at = newest.entry((server(a), &a.project)).or_insert(0);
        *at = a.last_activity.max(*at);
    }
    let latest = |a: &AgentView| Reverse(newest.get(&(server(a), &a.project)).copied());
    roots.sort_by_key(|a| (server(a), latest(a), &a.project, Reverse(a.last_activity)));
    let words: Vec<_> = g.query.split_whitespace().map(str::to_lowercase).collect();
    let (mut out, mut place, mut group) = (Vec::new(), None, 0);
    for root in roots {
        let mut shown = Vec::new();
        if !tree((app, &words, now), &kids, (root, 0), &mut shown) {
            continue;
        }
        let here = (server(root), &root.project);
        if place.is_none_or(|(s, _)| s != here.0) {
            let words = span(status::location(app, &root.session, columns), WHITE);
            out.push(Row::head(words, group));
        }
        if place != Some(here) {
            group += 1;
            out.push(Row::head(span(format!("  {}", root.project), CYAN), group));
        }
        place = Some(here);
        out.extend(
            shown
                .into_iter()
                .map(|row| agent_row(app, row, columns, group)),
        );
    }
    if out.is_empty() {
        out.push(Row::head(span("No agents match.", GRAY), group));
    }
    out.extend(history(app, g, columns, group + 1));
    out
}

// The history group, numbered `group`, while words are typed: its head saying how the search
// went, then a row for each conversation it found.
fn history(app: &App, g: &Goto, columns: usize, group: usize) -> Vec<Row> {
    let words = g.query.trim();
    if words.is_empty() {
        return Vec::new();
    }
    let answer = g.history.as_ref().filter(|h| h.0 == words).map(|h| &h.1);
    let said = match answer {
        None => "searching…".into(),
        Some(Err(e)) => e.to_string(),
        Some(Ok(f)) => {
            let partial = [" · a server did not answer", ""][usize::from(!f.partial)];
            format!("{} found{partial}", f.hits.len())
        }
    };
    let head = vec![span("HISTORY", WHITE), span(format!(" · {said}"), GRAY)];
    let mut out = vec![Row::head(head, group)];
    for hit in answer.into_iter().flatten().flat_map(|f| &f.hits) {
        let title = format!("    {}", cell(name(&hit.name, &hit.session), 30));
        let words = Line::from(vec![span(title, WHITE), span(hit.snippet.clone(), GRAY)]);
        let at = status::location(app, &hit.session, columns / 2);
        let when = clock(hit.at, app.zone.as_ref());
        let place = format!("{at} · {} · {when}", hit.cwd.display());
        out.push(Row {
            words,
            session: Some(hit.session.clone()),
            group,
            matched: true,
            history: true,
            place,
        });
    }
    out
}

/// Draws the popup over `area`, four fifths of it and at most 120 columns, edge to edge on a
/// phone; the search line's cursor while it types.
pub fn draw(app: &mut App, b: &mut Buffer, area: Rect, now: u64) -> Option<(u16, u16)> {
    if area.width < 24 || area.height < 8 {
        return None;
    }
    let g = app.goto.as_ref()?;
    let (w, h) = match area.width < super::view::PHONE {
        true => (area.width, area.height),
        false => ((area.width * 4 / 5).min(120), (area.height * 4 / 5).max(8)),
    };
    let (x, y) = ((area.width - w) / 2, (area.height - h) / 2);
    let r = Rect::new(area.x + x, area.y + y, w, h);
    let r = panel::frame(b, r, "Go to", CYAN);
    let rows = rows(app, g, r.width as usize, now);
    let total = app.agents.values().filter(|a| a.settled.is_none()).count();
    let agent = |r: &&Row| r.matched && r.session.is_some() && !r.history;
    let matched = rows.iter().filter(agent).count();
    let count = match g.query.trim().is_empty() {
        true => format!("{total} agent{}", plural(total)),
        false => format!("{matched} of {total}"),
    };
    let (search, ink) = match (g.typing, g.query.is_empty()) {
        (false, true) => ("/ search agents and history".to_owned(), GRAY),
        _ => (format!("/ {}", g.query), WHITE),
    };
    let gray = style(GRAY, GLASS);
    let room = r.width.saturating_sub(width(&count) as u16);
    let ink = style(ink, GLASS);
    let end = put(b, r.x, r.y, &search, room.saturating_sub(1), ink);
    put(b, r.x + room, r.y, &count, r.width, gray);
    let cursor = g.typing.then_some((end, r.y));
    // The highlight stays on its row while the row is drawn, else the first match takes it.
    let picks = rows
        .iter()
        .filter_map(|r| Some(((r.session.clone()?, r.history), r.group)));
    let picks: Vec<_> = picks.collect();
    let first = rows
        .iter()
        .find(|r| r.matched)
        .and_then(|r| Some((r.session.clone()?, r.history)));
    let g = app.goto.as_mut()?;
    if !picks.iter().any(|p| Some(&p.0) == g.selected.as_ref()) {
        g.selected = first.or_else(|| picks.first().map(|p| p.0.clone()));
    }
    let body = Rect::new(r.x, r.y + 2, r.width, r.height.saturating_sub(4));
    let height = (body.height as usize).max(1);
    let selected = |row: &Row| {
        row.session.as_ref().zip(Some(row.history)) == g.selected.as_ref().map(|(id, h)| (id, *h))
    };
    let at = rows.iter().position(|r| r.session.is_some() && selected(r));
    let at = at.unwrap_or(0);
    g.scroll = g.scroll.min(at).max((at + 1).saturating_sub(height));
    g.scroll = g.scroll.min(rows.len().saturating_sub(height));
    for (y, row) in (body.y..body.bottom()).zip(rows.iter().skip(g.scroll)) {
        if row.session.is_none() || !selected(row) {
            line(b, body, y, &row.words, style(DATA, GLASS));
            continue;
        }
        let band = Rect::new(r.x - 1, y, r.width + 2, 1);
        fill(b, band, style(WHITE, YOU_BG));
        line(b, body, y, &row.words, style(DATA, YOU_BG));
        put(b, r.x, y, "❯", 1, bold(CYAN, YOU_BG));
    }
    g.picks = picks;
    // The highlighted row's place, as the top bar says it, then the keys.
    let place = rows.get(at).filter(|row| selected(row));
    let place = place.map_or("", |row| &row.place);
    put(b, r.x, r.bottom() - 2, place, r.width, gray);
    let keys = "<goto.up>/<goto.down> rows · <goto.left>/<goto.right> project · \
                <goto.search> search · <goto.accept> open · <goto.cancel> close";
    let keys = bindings::hints(app, keys, " · ");
    put(b, r.x, r.bottom() - 1, &keys, r.width, gray);
    cursor
}
