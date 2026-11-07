//! The agents overview: every agent of the selected view's groups over the whole screen as an
//! instrument panel, a tile each, what it is for, what it is doing now and its helpers' marks,
//! from the list's own layout; and the fold between it and the list beside a session.
use super::panel::{self, Standing, on, standing};
use super::sections::{subtree, totals};
use super::tiles::{self, Group, Overview, Placed, Roots, frame};
use crate::protocol::views::{Section, SectionKind};
use crate::protocol::{AgentView, SessionId, now};
use crate::terminal::motion::{self, Moving, ease_out, progress};
use crate::terminal::text::*;
use crate::terminal::view::{self, Action, Hit, tint};
use crate::terminal::{App, bindings, servers};
use ratatui::style::{Style, Stylize};
use ratatui::{buffer::Buffer, layout::Rect};

/// How long the fold takes: about ten frames at `motion::MOVE`.
const FOLD: u64 = 320;

/// Whether the overview is shown and still, so that nothing beneath it is drawn.
pub fn covers(app: &App, now: u64) -> bool {
    app.list.overview.is_some() && folded(app, now).is_none()
}

/// Draws the overview over the frame beneath while it is shown or folding, from the fold's start
/// and `now` alone; true while it does, so the frame shows no cursor. Mid-fold its right edge
/// stands between the screen's right and the list's border (the left edge with the list closed):
/// behind it the overview fades out, then the list fades in, and the selection's tint travels
/// between the agent's block and its card.
pub fn draw(app: &mut App, b: &mut Buffer, now: u64, hits: &mut Vec<Hit>) -> bool {
    let shown = app.list.overview.is_some();
    let Some(t) = folded(app, now) else {
        if matches!(app.moving, Some(Moving::Fold(_))) {
            app.moving = None;
        }
        if shown {
            paint(app, b, now, hits);
        }
        return shown;
    };
    app.next.at(now);
    let (area, ground) = (b.area, style(DATA, GLASS));
    let (mut over, mut own) = (Buffer::empty(area), Vec::new());
    paint(app, &mut over, now, &mut own);
    let border = (app.list.area.filter(|_| app.list.open)).map_or(area.x, |r| r.right() - 1);
    let edge = (f64::from(area.right()) - t * f64::from(area.right() - border)).round() as u16;
    let list = Rect::new(0, 0, border, area.height);
    let passed = Rect {
        width: edge,
        ..list
    };
    if t < 0.45 {
        passed.positions().for_each(|at| b[at] = over[at].clone());
        tint(b, passed, GLASS, GLASS, t / 0.45);
    } else {
        tint(b, list, GLASS, GLASS, (1. - t) / 0.55);
        fill(b, Rect::new(border, 0, edge - border, area.height), ground);
    }
    for y in (0..area.height).filter(|_| edge < area.right()) {
        put(b, edge, y, "│", 1, style(CYAN, GLASS));
    }
    if let (Some(Action::Agent(chosen)), Some(id)) = (app.list.selected.clone(), app.focus.clone())
        && let (Some(from), Some(to)) = (block(&own, &chosen), block(hits, &id))
    {
        let at = |a: u16, b: u16| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u16;
        let (x, y) = (at(from.x, to.x), at(from.y, to.y));
        let glide = Rect::new(x, y, at(from.width, to.width), at(from.height, to.height));
        let tinted = Style::new().bg(mix(CYAN_BG, YOU_BG, t));
        b.set_style(glide.intersection(area), tinted);
    }
    if shown {
        *hits = own;
    }
    true
}

/// Starts the fold toward the side now shown, on from where a fold under way stands.
pub(super) fn fold(app: &mut App) {
    let (now, shown) = (now(), app.list.overview.is_some());
    // Read with its old start, a fold under way runs the other way.
    let stood = folded(app, now).map_or(f64::from(u8::from(shown)), |t| 1. - t);
    let travelled = 1. - [1. - stood, stood][usize::from(shown)].cbrt();
    app.moving = Some(Moving::Fold(now - (travelled * FOLD as f64) as u64));
}

// How far the fold stands at `now`, eased out: 0 at the overview, 1 at the view beneath; none
// once it has landed, with no fold moving, and whenever motion is off or the screen is under 80
// columns, so a fold under way lands on that frame.
fn folded(app: &App, now: u64) -> Option<f64> {
    let Some(Moving::Fold(at)) = app.moving else {
        return None;
    };
    let t = ease_out(progress(now, at, FOLD));
    let moves = now < at + FOLD && motion::on(app) && app.columns >= 80;
    moves.then_some([t, 1. - t][usize::from(app.list.overview.is_some())])
}

// The rows a frame drew for `id`'s agent: the first run of its hits, each under the one before.
fn block(hits: &[Hit], id: &SessionId) -> Option<Rect> {
    let mine = |(r, a): &Hit| matches!(a, Action::Agent(s) if s == id).then_some(*r);
    let runs = |run: Rect, r: Rect| [run, run.union(r)][usize::from(r.y == run.bottom())];
    hits.iter().filter_map(mine).reduce(runs)
}

/// The top-level groups the overview and the list number, in order: the selected view's groups
/// and Everything else holding any agent, each with its subgroups.
pub(super) fn groups(app: &App) -> Vec<&[Section]> {
    let all = app.list.layout().map_or(&[][..], |l| &l.sections);
    let held = |t: &&[Section]| panel::rows(t).any(|r| app.agents.contains_key(&r.session));
    let tops = (0..all.len()).filter(|i| all[*i].depth == 0 && grouped(&all[*i]));
    tops.map(|i| subtree(&all[i..])).filter(held).collect()
}
fn grouped(s: &Section) -> bool {
    matches!(s.kind, SectionKind::Group | SectionKind::EverythingElse)
}

/// Folds or unfolds the top-level group the overview lists `id` in.
pub(super) fn group(app: &mut App, id: &SessionId) {
    let holds = |t: &&[Section]| panel::rows(t).any(|r| r.session == *id);
    let tree = groups(app).into_iter().find(holds);
    let group = tree.map(|t| Action::Section(t[0].id.clone()));
    group.into_iter().for_each(|g| super::press(app, g));
}

/// The header's rows.
const HEAD: u16 = 4;

// The overview: its header, its groups from the scroll, and the residents and keys or a notice
// at its foot. While it is shown its stops are the list's.
fn paint(app: &mut App, b: &mut Buffer, now: u64, hits: &mut Vec<Hit>) {
    let (area, shown) = (b.area, app.list.overview.is_some());
    let resized = std::mem::replace(&mut app.list.sized, area) != area;
    let reveal = std::mem::take(&mut app.list.reveal) | resized | !shown;
    fill(b, area, style(DATA, PANEL));
    let trees = groups(app);
    let roots = trees
        .iter()
        .flat_map(|t| panel::rows(t))
        .filter(|r| r.depth == 0);
    let listed: Vec<_> = roots.filter_map(|r| app.agents.get(&r.session)).collect();
    header(app, b, &listed, now);
    let room = area.height.saturating_sub(HEAD + 2) as usize;
    let o = Overview {
        groups: build(app, &trees),
        w: area.width as usize,
        room,
    };
    let placed = o.fit();
    let scroll = app.list.overview.unwrap_or(0);
    let (scroll, stops, selected) = select(app, &o, &placed, scroll, reveal);
    // Drawn in body rows on a sheet of the rows in sight, only what crosses them, then laid
    // under the header.
    let sight = Rect::new(0, scroll as u16, area.width, room as u16);
    let mut sheet = Buffer::empty(sight);
    fill(&mut sheet, sight, style(DATA, PANEL));
    let seen = |r: &Rect| r.intersects(sight);
    for (i, at) in placed.groups.iter().enumerate().filter(|g| seen(&g.1.0)) {
        let lit = selected == Some(Action::Section(o.groups[i].tree[0].id.clone()));
        tiles::group(app, &mut sheet, (&o.groups[i], i, lit), *at);
    }
    for (r, g, j) in placed.panels.iter().filter(|p| seen(&p.0)) {
        tiles::shelf(&mut sheet, *r, o.groups[*g].tone, &o.groups[*g].panels[*j]);
    }
    for (r, g, a, below) in placed.tiles.iter().filter(|t| seen(&t.0)) {
        let lit = selected == Some(Action::Agent(a.session.clone()));
        let look = (o.groups[*g].tone, placed.size);
        tiles::tile(app, &mut sheet, *r, (a, below), look, lit, now);
    }
    if o.groups.is_empty() {
        let none = on("No agents in this view yet.", PANEL_GRAY, PANEL, false);
        panel::write(&mut sheet, 2, 0, &[none]);
    }
    let top = HEAD + 1;
    for at in sight.positions() {
        b[(at.x, top + at.y - sight.y)] = std::mem::take(&mut sheet[at]);
    }
    let seen = |r: Rect| {
        let y0 = (r.y as usize).max(scroll);
        let h = (r.bottom() as usize).min(scroll + room).checked_sub(y0)?;
        let r = Rect::new(r.x, top + (y0 - scroll) as u16, r.width, h as u16);
        (h > 0).then_some(r)
    };
    let tiles = placed.tiles.iter();
    let tiles = tiles.map(|t| (t.0, Action::Agent(t.2.session.clone())));
    let edge = |(r, _): &(Rect, u16)| Rect { height: 1, ..*r };
    let edges = placed.groups.iter().zip(&o.groups);
    let edges = edges.map(|(r, g)| (edge(r), Action::Section(g.tree[0].id.clone())));
    hits.extend(tiles.chain(edges).filter_map(|(r, a)| Some((seen(r)?, a))));
    if shown {
        (app.list.overview, app.list.rows, app.list.selected) = (Some(scroll), stops, selected);
    }
    foot(app, b, now);
}

// The groups as drawn, from their trees.
fn build<'a>(app: &'a App, trees: &[&'a [Section]]) -> Vec<Group<'a>> {
    let roots = |s: &'a Section| {
        let rows = s.sessions.iter().enumerate().filter(|(_, r)| r.depth == 0);
        let rows = rows.map(|(at, r)| Some((app.agents.get(&r.session)?, &s.sessions[at + 1..])));
        let mut own: Roots = rows.flatten().collect();
        own.sort_by_key(|(a, _)| standing(a));
        own
    };
    let group = |tree: &&'a [Section]| {
        let panels = tree.iter().enumerate();
        let panels = panels.map(|(i, s)| ((i > 0).then_some(s.name.as_str()), roots(s)));
        let panels = panels.filter(|p| !p.1.is_empty()).collect();
        let tone = panel::tone(tree[0].colour);
        let folded = app.list.toggled.contains(&tree[0].id);
        Group {
            tree,
            tone,
            folded,
            panels,
        }
    };
    trees.iter().map(group).collect()
}

// The stops in order (each tile, and each folded group), the selection moved onto one, and the
// scroll that shows it when `reveal` or when it moved: an agent a fold hides moves to its group,
// an open group to its first tile.
type Selected = (usize, Vec<Action>, Option<Action>);
fn select(app: &App, o: &Overview, placed: &Placed, scroll: usize, reveal: bool) -> Selected {
    let section = |g: &Group| Action::Section(g.tree[0].id.clone());
    let mut stops: Vec<(Action, Rect)> = Vec::new();
    for (i, g) in o.groups.iter().enumerate() {
        let tiles = placed.tiles.iter().filter(|t| t.1 == i && !g.folded);
        stops.extend(g.folded.then(|| (section(g), placed.groups[i].0)));
        stops.extend(tiles.map(|t| (Action::Agent(t.2.session.clone()), t.0)));
    }
    let selected = app.list.selected.clone();
    let holds = |g: &&Group, id: &SessionId| panel::rows(g.tree).any(|r| r.session == *id);
    let moved = match &selected {
        Some(a @ Action::Agent(id)) if !stops.iter().any(|s| s.0 == *a) => o
            .groups
            .iter()
            .find(|g| g.folded && holds(g, id))
            .map(section),
        Some(Action::Section(id)) => {
            let open = o
                .groups
                .iter()
                .position(|g| g.tree[0].id == *id && !g.folded);
            let first = open.and_then(|i| placed.tiles.iter().find(|t| t.1 == i));
            first.map(|t| Action::Agent(t.2.session.clone()))
        }
        _ => None,
    };
    let chosen = moved.or(selected.clone());
    let (mut scroll, moved) = (scroll, reveal || chosen != selected);
    let at = stops
        .iter()
        .find(|s| Some(&s.0) == chosen.as_ref())
        .filter(|_| moved);
    if let Some((_, r)) = at {
        let bottom = (r.bottom() as usize).saturating_sub(o.room);
        scroll = scroll.min(r.y as usize).max(bottom);
    }
    let scroll = scroll.min((placed.bottom + 1).saturating_sub(o.room));
    (scroll, stops.into_iter().map(|s| s.0).collect(), chosen)
}

// The header: the clock's panel from 100 columns, the needs-you panel, and the fleet's from 150.
fn header(app: &App, b: &mut Buffer, listed: &[&AgentView], now: u64) {
    let (w, census) = (b.area.width, panel::census(listed.iter().copied()));
    let day = zoned(now, app.zone.as_ref()).map(|t| t.strftime("%A|%d %b|%P").to_string());
    let day = day.unwrap_or_default().to_uppercase();
    let day: Vec<_> = day.split('|').chain(["", "", ""]).collect();
    let date = format!("{} · {}", day[1], panel::count(listed.len(), "AGENT"));
    let fleet = [0, 52][usize::from(w >= 150)];
    let clock = (24 + width(&date) as u16).min([40, 99][usize::from(fleet > 0)]);
    let clock = clock * u16::from(w >= 100);
    let (ink, dim) = ((PANEL_INK, PANEL), |t| on(t, PANEL_GRAY, PANEL, false));
    if clock > 0 {
        frame(b, Rect::new(1, 0, clock, HEAD), ink, true, None);
        let tabs = [(" RAZOR ", CHIP_INK, CYAN), (" AGENTS ", CYAN, CLOCK_BG)];
        panel::write(b, 3, 0, &panel::tabs(&tabs, PANEL));
        let x = panel::clock(app, b, 4, 1, now) + 1;
        panel::write(b, x, 1, &[dim(day[2].to_lowercase())]);
        let seconds = panel::seconds(app, now).unwrap_or_default();
        panel::write(b, x, 2, &[on(seconds, MAGENTA, PANEL, true)]);
        let room = (clock + 1).saturating_sub(x + 6) as usize;
        panel::write(b, x + 4, 1, &[on(clip(day[0], room), WHITE, PANEL, true)]);
        panel::write(b, x + 4, 2, &[dim(clip(&date, room))]);
    }
    if fleet > 0 {
        let x = w - 1 - fleet;
        frame(b, Rect::new(x, 0, fleet, HEAD), ink, true, None);
        let tab = [(" FLEET ", CHIP_INK, mix(WHITE, PANEL_INK, 0.25))];
        panel::write(b, x + 2, 0, &panel::tabs(&tab, PANEL));
        let helpers = panel::count(groups(app).iter().map(|t| totals(t).1).sum(), "HELPER");
        let servers = panel::count(app.servers.len(), "SERVER");
        let tail = format!(" {helpers} · {servers} ");
        let at = w - 3 - width(&tail) as u16;
        let tail = on(tail, mix(PANEL_INK, WHITE, 0.4), PANEL, false);
        panel::write(b, at, HEAD - 1, &[tail]);
        let names = ["ASK", "STUCK", "WORK", "IDLE"]
            .into_iter()
            .zip(panel::CENSUS);
        let words = names.zip(census).flat_map(|((name, (mark, ink)), n)| {
            let (mark, n) = (format!("{mark} "), on(n.to_string(), ink, PANEL, true));
            [on(mark, ink, PANEL, false), n, dim(format!(" {name}   "))]
        });
        let room = fleet as usize - 4;
        panel::write(b, x + 2, 1, &clip_spans(words.collect(), room));
        panel::write(b, x + 2, 2, &panel::bar(census, room, PANEL));
    }
    let x = 1 + clock + u16::from(clock > 0);
    let right = (w - 1).saturating_sub(fleet + u16::from(fleet > 0));
    needs(
        app,
        b,
        Rect::new(x, 0, right.saturating_sub(x), HEAD),
        listed,
        now,
    );
}

// The needs-you panel in `r`: the oldest that needs the person as a solid bar in its state's
// colour, the next numbered under it, and how many more on its foot; with none, a calm line.
fn needs(app: &App, b: &mut Buffer, r: Rect, listed: &[&AgentView], now: u64) {
    frame(b, r, (PANEL_INK, PANEL), true, None);
    let since = |a: &AgentView| a.asks.first().map_or(a.last_activity, |q| q.at);
    let mut need: Vec<_> = listed.iter().filter(|a| standing(a).needs()).collect();
    need.sort_by_key(|a| (standing(a) != Standing::Asks, since(a)));
    let (n, w) = (need.len(), r.width.saturating_sub(4) as usize);
    let count = format!(" ◆ {n} ");
    let mut tabs = vec![(count.as_str(), CHIP_INK, AMBER)];
    tabs.push((" NEED YOU ", AMBER, mix(GLASS, AMBER, 0.2)));
    if n == 0 {
        tabs = vec![(" NEED YOU ", PANEL_GRAY, mix(PANEL, PANEL_INK, 0.5))];
        let calm = on("Nothing waits on you.", PANEL_GRAY, PANEL, false);
        panel::write(b, r.x + 2, 1, &[calm]);
    }
    panel::write(b, r.x + 2, 0, &panel::tabs(&tabs, PANEL));
    if n > 2 {
        let more = on(format!(" +{} MORE ", n - 2), AMBER, PANEL, false);
        panel::write(b, r.right() - 16, HEAD - 1, &[more]);
    }
    for (i, a) in need.into_iter().take(2).enumerate() {
        let (s, age) = (standing(a), panel::age(app, since(a), now));
        let (ground, words) = ([s.mark().1, PANEL][i], s.said(a).2);
        let (mark, name) = (format!("{} ", s.mark().0), format!("{}  ", a.name));
        let left = match i {
            0 => vec![on(format!(" ▶ {name}"), CHIP_INK, ground, true)],
            _ => vec![
                on(" 02 ", AMBER, PANEL, true),
                on(mark, s.mark().1, PANEL, false),
            ],
        };
        let words = match i {
            0 => vec![on(words, CHIP_INK, ground, false)],
            _ => vec![
                on(name, WHITE, PANEL, true),
                on(words, ASK_WORDS, PANEL, false).italic(),
            ],
        };
        let age = on(format!("{age} "), [CHIP_INK, PANEL_GRAY][i], ground, i == 0);
        let line = panel::ends([left, words].concat(), vec![age], w, ground);
        panel::write(b, r.x + 2, 1 + i as u16, &line);
    }
}

// The foot row: any server not answering, then the residents and their next wakes on the left,
// the keys on the right, or a notice.
fn foot(app: &App, b: &mut Buffer, now: u64) {
    let (w, y) = (b.area.width, b.area.height.saturating_sub(1));
    let silent = servers::silent(app, now).map(|s| on(format!("{s}   "), AMBER, PANEL, true));
    let used = silent.map_or(0, |s| panel::write(b, 1, y, &[s]) - 1);
    let mut names = vec![on("RESIDENTS  ", PANEL_GRAY, PANEL, true)];
    for r in app.servers.iter().flat_map(|s| &s.residents) {
        let wake = r.upcoming.first().map(|(_, at)| servers::hhmm(app, *at));
        let wake = format!(" {}     ", wake.unwrap_or_default());
        names.extend([
            on(r.session.to_string(), DATA, PANEL, false),
            on(wake, PANEL_GRAY, PANEL, false),
        ]);
    }
    let used = match names.len() > 1 {
        true => panel::write(b, 1 + used, y, &names),
        false => used,
    };
    let keys = "<list.group_1>-<list.group_9> group   <list.up> <list.down> move   \
        <list.accept> open   <list.fold> fold   <list.close> back";
    let keys = bindings::text(app, keys);
    let at = w.saturating_sub(width(&keys) as u16 + 2);
    if at > used + 10 || used == 0 {
        panel::write(b, at, y, &[on(keys, PANEL_GRAY, PANEL, false)]);
    }
    view::notice(app, b, Rect::new(0, y, w, 1), now);
}
