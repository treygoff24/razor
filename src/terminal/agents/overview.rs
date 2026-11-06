//! The agents overview: every agent of the selected view's groups over the whole screen, what
//! it is for, what it is doing now and its helpers beside it, from the list's own layout; and
//! the fold between it and the list beside a session.
use super::sections::{counts, subtree, totals};
use crate::protocol::views::{Section, SectionKind, ViewRow};
use crate::protocol::{AgentView, SessionId, State, now};
use crate::terminal::cards::{self, Card, Size};
use crate::terminal::motion::{self, Moving, ease_out, progress};
use crate::terminal::status;
use crate::terminal::text::*;
use crate::terminal::view::{self, Action, Hit, tint};
use crate::terminal::{App, bindings, servers, tools};
use ratatui::{buffer::Buffer, layout::Rect, style::Color, style::Style, text::Line, text::Span};

type Spans = Vec<Span<'static>>;

/// How long the fold takes: about ten frames at `motion::MOVE`.
const FOLD: u64 = 320;
/// Each standing's mark, ink and census words for one agent and for more: wanting the person,
/// stuck, working and idle.
const STANDING: [(&str, Color, [&str; 2]); 4] = [
    ("◆", AMBER, ["wants you", "want you"]),
    ("▲", RED, ["stuck"; 2]),
    ("●", GREEN, ["working"; 2]),
    ("○", GRAY, ["idle"; 2]),
];

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

// The overview: its bar, the columns' heads, its body, and the keys or a notice at its foot.
// While it is shown its stops are the list's.
fn paint(app: &mut App, b: &mut Buffer, now: u64, hits: &mut Vec<Hit>) {
    let (area, shown, w) = (b.area, app.list.overview.is_some(), b.area.width as usize);
    let resized = std::mem::replace(&mut app.list.sized, area) != area;
    let reveal = std::mem::take(&mut app.list.reveal) | resized | !shown;
    fill(b, area, style(DATA, GLASS));
    let listed: Vec<_> = (sections(app).iter().filter(|s| grouped(s)))
        .flat_map(|s| s.sessions.iter().filter(|r| r.depth == 0))
        .filter_map(|r| app.agents.get(&r.session))
        .collect();
    let c = columns(app, &listed, w);
    let census = census(listed.into_iter());
    let answer = app.servers.iter().filter(|s| s.silent.is_none()).count();
    let clock = zoned(now, app.zone.as_ref()).map(|t| t.strftime("%a %H:%M").to_string());
    let clock = clock.unwrap_or_default();
    let middle = w.saturating_sub(census.iter().map(|s| width(&s.content)).sum()) / 2;
    let brand = format!(" {} ", app.brand.name.to_uppercase());
    let bar = vec![
        Span::styled(brand, bold(GLASS, CYAN)),
        Span::styled("  agents", bold(WHITE, GLASS)),
        span(format!("    {answer} server{}", plural(answer)), GRAY),
    ];
    let right = w.saturating_sub(width(&clock));
    let bar = vec![(0, bar), (middle, census), (right, vec![span(clock, GRAY)])];
    let heads = ["AGENT", "SERVER", "PROJECT", "GOAL", "HELPERS", "LAST"];
    let at = [c[1], c[2], c[3], c[4], c[6], w.saturating_sub(4)];
    let heads = (at.into_iter().zip(heads)).map(|(x, s)| (x, vec![span(s, GRAY_DIM)]));
    for (y, row) in [(0, placed(w, bar)), (2, placed(w, heads.collect()))] {
        line(b, area, y, &row, style(DATA, GLASS));
    }
    let mut o = Overview {
        app,
        c,
        w,
        tight: 2,
        now,
    };
    let body = Rect::new(0, 4, area.width, area.height.saturating_sub(5));
    let (scroll, stops, selected) = o.body(b, body, app.list.overview.unwrap_or(0), reveal, hits);
    if shown {
        (app.list.overview, app.list.rows, app.list.selected) = (Some(scroll), stops, selected);
    }
    let keys = "<list.up> <list.down> move   <list.right> <list.accept> open   \
        <list.fold> fold a group   <list.close> <global.overview> back to the session";
    let (keys, foot) = (bindings::text(app, keys), area.height.saturating_sub(1));
    put(
        b,
        4,
        foot,
        &keys,
        w.saturating_sub(4) as u16,
        style(GRAY_DIM, GLASS),
    );
    view::notice(app, b, Rect::new(0, foot, area.width, 1), now);
}

/// Folds or unfolds the group the overview lists `id` in.
pub(super) fn group(app: &mut App, id: &SessionId) {
    let mut groups = sections(app).iter().filter(|s| grouped(s));
    let group = groups.find(|s| s.sessions.iter().any(|r| r.session == *id));
    let group = group.map(|s| Action::Section(s.id.clone()));
    group.into_iter().for_each(|g| super::press(app, g));
}

// The sections the overview lists: the selected view's groups and Everything else.
fn sections(app: &App) -> &[Section] {
    app.list.layout().map_or(&[], |l| &l.sections)
}
fn grouped(s: &Section) -> bool {
    matches!(s.kind, SectionKind::Group | SectionKind::EverythingElse)
}

// 0 wants the person (a question, or a turn that ended in an error), 1 stuck, 2 working, 3 idle.
fn standing(a: &AgentView) -> usize {
    match a.state {
        _ if !a.asks.is_empty() => 0,
        State::Stuck => 1,
        State::Working => 2,
        _ if a.error.is_some() => 0,
        _ => 3,
    }
}

// The agents counted by standing, each count in its standing's ink.
fn census<'a>(agents: impl Iterator<Item = &'a AgentView>) -> Spans {
    let mut n = [0; 4];
    agents.for_each(|a| n[standing(a)] += 1);
    let mut out = Vec::new();
    for (n, (_, ink, words)) in n.into_iter().zip(STANDING).filter(|(n, _)| *n > 0) {
        out.extend((!out.is_empty()).then(|| span("  ·  ", GRAY_DIM)));
        out.push(span(format!("{n} {}", words[usize::from(n > 1)]), ink));
    }
    out
}

// The owning server's label.
fn label<'a>(app: &'a App, a: &AgentView) -> &'a str {
    status::server(app, &a.session).map_or("", |s| s.label.as_str())
}

// Where each run starts at `w` columns: the mark, name, server, project, goal, status text and
// helpers. The name, server and project columns fit their heads and the widest of `agents`, at
// most an eighth, a ninth and a ninth of the width; the helpers take the last quarter.
fn columns(app: &App, agents: &[&AgentView], w: usize) -> [usize; 7] {
    let mut most = [5, 6, 7];
    for a in agents {
        for (m, s) in most.iter_mut().zip([&a.name, label(app, a), &a.project]) {
            *m = width(s).max(*m);
        }
    }
    let shares = [(most[0], 8), (most[1], 9), (most[2], 9)];
    let [name, server, project] = shares.map(|(most, share)| most.min(w.div_ceil(share)) + 2);
    let goal = 7 + name + server + project;
    [4, 7, 7 + name, 7 + name + server, goal, goal + 7, w - w / 4]
}

// Runs of spans at their columns, left to right, each cut a column short of the next run's start
// and the last at `w`; a run that would start inside the one before it starts after it.
fn placed(w: usize, runs: Vec<(usize, Spans)>) -> Line<'static> {
    let ends: Vec<_> = (runs.iter().skip(1))
        .map(|r| r.0.saturating_sub(1))
        .chain([w])
        .collect();
    let (mut out, mut x) = (Vec::new(), 0);
    for ((at, spans), end) in runs.into_iter().zip(ends) {
        let at = at.max(x);
        let spans = clip_spans(spans, end.saturating_sub(at));
        out.push(span(" ".repeat(at - x), GRAY));
        x = at + spans.iter().map(|s| width(&s.content)).sum::<usize>();
        out.extend(spans);
    }
    Line::from(out)
}

// One overview laid out at width `w`: where each run starts (`columns`), and how tightly its
// body is laid: 0 full blocks a blank row apart, 1 full blocks a blank row apart only between
// groups, 2 a line per agent.
struct Overview<'a> {
    app: &'a App,
    c: [usize; 7],
    w: usize,
    tight: u8,
    now: u64,
}

// A block of the body: a group's heading with the sections under it, a root with its layout's
// rows after it, or the residents' line.
enum Block<'a> {
    Heading(&'a [Section]),
    Agent(&'a AgentView, &'a [ViewRow]),
    Residents,
}

// Blank rows between block `a` and the next, `b`, at `tight`: after an agent one before a
// heading, and one more when loose; a heading and the first thing it holds none, or one when
// loose under the first depth; one between any other headings.
fn gap(a: &Block, b: &Block, tight: u8) -> usize {
    let loose = usize::from(tight == 0);
    let depth = |b: &Block| match b {
        Block::Heading(tree) => Some(tree[0].depth),
        Block::Agent(..) => None,
        Block::Residents => Some(0),
    };
    match (depth(a), depth(b)) {
        (None, b) => loose + usize::from(b.is_some()),
        (Some(d), e) if e.is_none_or(|e| e > d) => loose * usize::from(d == 0),
        _ => 1,
    }
}

// How many rows of `room` columns `words` wraps to, at most three.
fn wrapped(words: &str, room: usize) -> usize {
    let mut rows = 0;
    visit_wrap(&[Span::raw(words)], room, |parts, _, last| {
        rows += usize::from(!last || !parts.is_empty());
        rows < 3
    });
    rows
}

impl<'a> Overview<'a> {
    // The body's blocks: each listed group at its depth, a folded one as its heading alone, and
    // its own agents most urgent first; then the residents.
    fn blocks(&self) -> Vec<Block<'a>> {
        let (app, all) = (self.app, sections(self.app));
        let (mut out, mut hidden) = (Vec::new(), None::<u16>);
        for (i, s) in all.iter().enumerate().filter(|(_, s)| grouped(s)) {
            if hidden.is_some_and(|depth| s.depth > depth) {
                continue;
            }
            let folded = app.list.toggled.contains(&s.id);
            hidden = folded.then_some(s.depth);
            out.push(Block::Heading(subtree(&all[i..])));
            if folded {
                continue;
            }
            let mut own: Vec<_> = (s.sessions.iter().enumerate())
                .filter(|(_, r)| r.depth == 0)
                .filter_map(|(at, r)| Some((app.agents.get(&r.session)?, &s.sessions[at + 1..])))
                .collect();
            own.sort_by_key(|(a, _)| [0, 0, 1, 2][standing(a)]);
            out.extend(own.into_iter().map(|(a, below)| Block::Agent(a, below)));
        }
        let homes = app.servers.iter().any(|s| !s.residents.is_empty());
        out.extend(homes.then_some(Block::Residents));
        out
    }

    // Where each block starts at `tight`, and where the last ends; none for a full form taller
    // than `height`.
    fn place(&self, blocks: &[Block], tight: u8, height: usize) -> Option<(Vec<usize>, usize)> {
        let (mut starts, mut y, mut last) = (Vec::with_capacity(blocks.len()), 0, None);
        for block in blocks {
            y += last.map_or(0, |last| gap(last, block, tight));
            starts.push(y);
            (y, last) = (y + self.tall(block, tight), Some(block));
            if tight < 2 && y > height {
                return None;
            }
        }
        Some((starts, y))
    }

    // Draws the body into `r`, laid as loosely as its height allows, else a line per agent (as
    // under 80 columns) from `scroll`, moved to show the selected block when `reveal`. Only the
    // blocks in sight are laid out. Returns the scroll it drew, its stops in order and the
    // selection, moved onto a stop.
    fn body(
        &mut self,
        b: &mut Buffer,
        r: Rect,
        mut scroll: usize,
        reveal: bool,
        hits: &mut Vec<Hit>,
    ) -> (usize, Vec<Action>, Option<Action>) {
        let (height, blocks) = (r.height as usize, self.blocks());
        let loosest = [0, 2][usize::from(self.w < 80)];
        let fit = (loosest..3).find_map(|t| Some((t, self.place(&blocks, t, height)?)));
        let (tight, (starts, end)) = fit.unwrap_or_default();
        self.tight = tight;
        let actions: Vec<_> = (blocks.iter())
            .map(|block| match block {
                Block::Heading(tree) => Some(Action::Section(tree[0].id.clone())),
                Block::Agent(a, _) => Some(Action::Agent(a.session.clone())),
                Block::Residents => None,
            })
            .collect();
        let (toggled, selected) = (&self.app.list.toggled, &self.app.list.selected);
        // The stops are the agents and the folded headings, which stand for the agents they hide.
        let stop = |i: &usize| match &actions[*i] {
            Some(Action::Section(id)) => toggled.contains(id),
            a => matches!(a, Some(Action::Agent(_))),
        };
        // A selection the body does not stop at moves to the next stop, else the last: what a
        // folded heading hides to the heading, an unfolded heading to its first agent.
        let holds = |i: &usize| match (&blocks[*i], selected) {
            (Block::Heading(tree), Some(a)) if stop(i) => tree.iter().any(|s| match a {
                Action::Agent(id) => s.sessions.iter().any(|r| r.session == *id),
                _ => matches!(a, Action::Section(id) if *id == s.id),
            }),
            _ => selected.is_some() && actions[*i] == *selected,
        };
        let held = (0..blocks.len()).find(holds);
        let chosen = held.and_then(|h| (h..blocks.len()).chain((0..h).rev()).find(stop));
        let moved = chosen.map_or(selected.clone(), |i| actions[i].clone());
        let at = (chosen.filter(|_| reveal || moved != *selected))
            .and_then(|i| blocks.get(i).zip(starts.get(i)));
        if let Some((block, top)) = at {
            scroll = (scroll.min(*top)).max((top + self.tall(block, tight)).saturating_sub(height));
        }
        let scroll = scroll.min(end.saturating_sub(height));
        for (i, (block, top)) in blocks.iter().zip(&starts).enumerate() {
            // Past the body nothing is in sight; a block whose next starts above the scroll is not.
            if *top >= scroll + height {
                break;
            }
            if starts.get(i + 1).is_some_and(|next| *next <= scroll) {
                continue;
            }
            for (j, words) in self.lines(block, chosen == Some(i)).iter().enumerate() {
                let Some(y) = (top + j).checked_sub(scroll).filter(|y| *y < height) else {
                    continue;
                };
                let row = Rect::new(r.x, r.y + y as u16, r.width, 1);
                line(b, row, row.y, words, style(DATA, GLASS));
                // The selected agent's block takes a cyan tint and, on its first row, the ❯.
                if chosen == Some(i) && matches!(block, Block::Agent(..)) {
                    let tint = Rect::new(1, row.y, r.width.saturating_sub(2), 1);
                    b.set_style(tint, Style::new().bg(CYAN_BG));
                    if j == 0 {
                        put(b, 2, row.y, "❯", 1, bold(CYAN, CYAN_BG));
                    }
                }
                hits.extend(actions[i].clone().map(|a| (row, a)));
            }
        }
        let stops = (0..blocks.len()).filter(stop).map(|i| actions[i].clone());
        (scroll, stops.flatten().collect(), moved)
    }

    // The rows `block` takes at `tight`, without laying them out: in a full form an agent's top
    // line and status lines, or its helpers' column if longer.
    fn tall(&self, block: &Block, tight: u8) -> usize {
        match block {
            Block::Agent(a, below) if tight < 2 => {
                let (shown, done) = self.listed(below);
                let lines = (shown.len() + usize::from(done > 0)).min(6);
                let room = self.room(lines);
                let text: usize = (said(a, self.now).iter())
                    .map(|(.., words)| wrapped(words, room))
                    .sum();
                lines.max(1 + text)
            }
            Block::Residents => 2,
            _ => 1,
        }
    }

    // The room status text has beside a helpers column of `lines` lines.
    fn room(&self, lines: usize) -> usize {
        [self.w, self.c[6] - 1][usize::from(lines > 0)].saturating_sub(self.c[5])
    }

    // The rows `block` draws, `chosen` or not.
    fn lines(&self, block: &Block, chosen: bool) -> Vec<Line<'static>> {
        let app = self.app;
        let tree = match block {
            Block::Heading(tree) => tree,
            Block::Agent(a, below) => return self.agent(a, below),
            Block::Residents => {
                let mut names = Vec::new();
                for r in app.servers.iter().flat_map(|s| &s.residents) {
                    names.extend((!names.is_empty()).then(|| span("     ", GRAY)));
                    names.push(span(r.session.to_string(), DATA));
                    let wake = r.upcoming.first().map(|(_, at)| servers::hhmm(app, *at));
                    names.extend(wake.map(|at| span(format!("  {at}"), GRAY)));
                }
                let head = self.heading(span("", GRAY), "RESIDENTS", 0, Vec::new());
                return vec![head, placed(self.w, vec![(self.c[1], names)])];
            }
        };
        let s = &tree[0];
        let tail = match (s.depth, totals(tree)) {
            (0, (agents, helpers)) => vec![span(counts(agents, helpers), GRAY)],
            _ => census(
                (tree.iter().flat_map(|s| &s.sessions))
                    .filter(|r| r.depth == 0)
                    .filter_map(|r| app.agents.get(&r.session)),
            ),
        };
        let mark = match (chosen, app.list.toggled.contains(&s.id)) {
            (true, _) => Span::styled("❯ ", bold(CYAN, GLASS)),
            (_, true) => span("▸ ", GRAY),
            _ => span("", GRAY),
        };
        let upper = s.name.to_uppercase();
        let name = [&s.name, &upper][usize::from(s.depth == 0)];
        vec![self.heading(mark, name, s.depth, tail)]
    }

    // A heading at its depth's indent: its mark and name, a rule (dimmer under the first depth),
    // and `tail` at the right.
    fn heading(&self, mark: Span<'static>, name: &str, depth: u16, tail: Spans) -> Line<'static> {
        let (indent, name) = (1 + 2 * depth as usize, format!("{name}  "));
        let used = width(&mark.content) + tail.iter().map(|s| width(&s.content)).sum::<usize>();
        let dashes = self.w.saturating_sub(indent + used + width(&name) + 2);
        let rule = "─".repeat(dashes) + "  ";
        let rule = span(rule, [GRAY, GRAY_DIM][usize::from(depth > 0)]);
        let mut spans = vec![mark, Span::styled(name, bold(WHITE, GLASS)), rule];
        spans.extend(tail);
        placed(self.w, vec![(indent, spans)])
    }

    // One root whose descendants' rows begin `below`: its top line, then in a full form its status
    // lines under the goal, each wrapped to the room its helpers' column leaves, beside that
    // column; out of it, one line holding its most urgent status line and its helpers' marks.
    fn agent(&self, a: &AgentView, below: &[ViewRow]) -> Vec<Line<'static>> {
        let (c, w, standing, full) = (self.c, self.w, standing(a), self.tight < 2);
        let ((mark, ink, _), idle) = (STANDING[standing], standing == 3);
        let strong = [bold(WHITE, GLASS), style(DATA, GLASS)][usize::from(idle)];
        let column = self.column(below);
        let (rows, mut column) = (column.len(), column.into_iter());
        let said = said(a, self.now);
        let goal = [&a.headline, &a.name][usize::from(a.headline.is_empty())];
        let mut goal = vec![Span::styled(goal.clone(), strong)];
        let after = match (idle, said.first()) {
            (true, _) if !a.last_reply.is_empty() => vec![span(a.last_reply.clone(), GRAY)],
            (false, Some((label, ink, text))) if !full => {
                let text = text.lines().next().unwrap_or_default().to_owned();
                vec![span(*label, *ink), span(" ", GRAY), span(text, DATA)]
            }
            _ => Vec::new(),
        };
        if !after.is_empty() {
            goal.push(span("  ·  ", GRAY_DIM));
            goal.extend(after);
        }
        let right: Option<Spans> = match full {
            true => column.next().map(|(_, l)| l.spans),
            false => (rows > 0).then(|| column.by_ref().map(|(m, _)| m).collect()),
        };
        let age = status::age(self.now.saturating_sub(a.last_activity));
        let mut top = vec![
            (c[0], vec![span(mark, ink)]),
            (c[1], vec![Span::styled(a.name.clone(), strong)]),
            (c[2], vec![span(label(self.app, a).to_owned(), GRAY)]),
            (c[3], vec![span(a.project.clone(), GRAY)]),
            (c[4], goal),
        ];
        top.extend(right.map(|spans| (c[6], spans)));
        let end = w.saturating_sub(width(&age) + 1);
        top.iter_mut()
            .for_each(|(at, _)| *at = (*at).min(end.saturating_sub(1)));
        top.push((end, vec![span(age, GRAY)]));
        if !full {
            return vec![placed(w, top)];
        }
        let room = self.room(rows);
        let mut text = Vec::new();
        for (label, ink, words) in said {
            let tone = [DATA, GRAY][usize::from(label == "step")];
            let mut rows = wrap(vec![span(words, tone)], room);
            if rows.len() > 3 {
                rows.truncate(3);
                let last: String = rows[2].spans.iter().map(|s| s.content.as_ref()).collect();
                rows[2] = Line::from(span(clip(&format!("{last}…"), room), tone));
            }
            let labels = std::iter::once(span(label, ink)).chain(std::iter::repeat(span("", ink)));
            text.extend(labels.zip(rows));
        }
        let rail = span("│", mix(GLASS, ink, 0.55));
        let mut lines = vec![placed(w, top)];
        for i in 1..rows.max(1 + text.len()) {
            let mut runs = vec![(c[0], vec![rail.clone()])];
            if let Some((label, words)) = text.get(i - 1) {
                runs.extend([(c[4], vec![label.clone()]), (c[5], words.spans.clone())]);
            }
            runs.extend(column.next().map(|(_, l)| (c[6], l.spans)));
            lines.push(placed(w, runs));
        }
        lines
    }

    // The helpers the column of the root whose descendants' rows begin `below` lists, each with
    // its depth, and how many finished at the top level fold into its done line.
    fn listed(&self, below: &[ViewRow]) -> (Vec<(usize, &'a AgentView)>, usize) {
        let (mut shown, mut done, mut finished) = (Vec::new(), 0, false);
        let rows = below.iter().take_while(|r| r.depth > 0);
        for (r, a) in rows.filter_map(|r| Some((r, self.app.agents.get(&r.session)?))) {
            if r.depth == 1 {
                finished = r.folded || cards::look(a).1 == "finished";
                done += usize::from(finished);
            }
            if !finished {
                shown.push((r.depth as usize, a));
            }
        }
        (shown, done)
    }

    // The helpers column of the root whose descendants' rows begin `below`: each listed helper's
    // mark and, in a full form, its card's top row as the list words it, trimmed to its mark,
    // branch, name and state; then the `✓ N done` line; at most six lines, the last `+N more`
    // when there are more. Only the cards it shows are read.
    fn column(&self, below: &[ViewRow]) -> Vec<(Span<'static>, Line<'static>)> {
        let (shown, done) = self.listed(below);
        let (room, full) = (self.w.saturating_sub(self.c[6] + 6), self.tight < 2);
        let mut depths: Vec<_> = shown.iter().map(|(depth, _)| *depth).collect();
        depths.extend((done > 0).then_some(1));
        let keep = [depths.len(), 5][usize::from(depths.len() > 6)];
        let branches = full.then(|| cards::connectors(&depths, false, room));
        let mut branches = branches.unwrap_or_default().into_iter();
        let (detail, mut lines) = (cards::detail(&[], room, Size::Mini), Vec::new());
        for (_, a) in shown.iter().take(keep) {
            let mut card = Card::of(self.app, a, self.now);
            card.clock.clear();
            let mut row = Line::default();
            if let Some(branch) = branches.next() {
                let room = room.saturating_sub(width(&branch) - 1);
                row = (card.rows(Size::Full, room, detail, (false, false)).next()).unwrap_or(row);
                let at = 3.min(row.spans.len());
                row.spans.insert(at, span(branch[1..].to_owned(), GRAY_DIM));
                row.spans.insert(0, span("│", GRAY_DIM));
            }
            lines.push((Span::styled(card.glyph, style(card.tone, GLASS)), row));
        }
        if done > 0 && shown.len() < keep {
            let branch = branches.next().map_or(String::new(), |b| b[1..].to_owned());
            let done = format!("{done} done");
            let words = [
                ("│", GRAY_DIM),
                (" ✓ ", GRAY),
                (&branch, GRAY_DIM),
                (&done, GRAY),
            ];
            let words = words.map(|(s, ink)| span(s.to_owned(), ink));
            lines.push((span("✓", GRAY), Line::from_iter(words)));
        }
        if keep < depths.len() {
            let more = depths.len() - keep;
            let line = Line::from(span(format!("│   +{more} more"), GRAY_DIM));
            lines.push((span(format!("+{more}"), GRAY_DIM), line));
        }
        lines
    }
}

// A root's status lines, most urgent first, each with its label and the label's ink: what it
// asks, why it is stuck or failed, its latest words while it works, and the call running now.
fn said(a: &AgentView, now: u64) -> Vec<(&'static str, Color, String)> {
    let working = a.state == State::Working;
    let failed = match a.state {
        State::Stuck => Some(("stuck", a.error.as_deref().unwrap_or(cards::STUCK))),
        State::Working => None,
        _ => a.error.as_deref().map(|e| ("error", e)),
    };
    let failed = failed.map(|(label, why)| (label, RED, why.to_owned()));
    let asks = a.asks.first().map(|q| ("asks", AMBER, q.text.clone()));
    let words =
        (working && !a.last_reply.is_empty()).then(|| ("now", VIOLET, a.last_reply.clone()));
    let step = (a.doing.as_ref().filter(|_| working)).map(|(tool, target, at)| {
        let mark = tools::mark(tool, &serde_json::Value::Null);
        let age = status::age(now.saturating_sub(*at));
        ("step", GRAY, format!("{mark} {target} · {age}"))
    });
    [asks, failed, words, step].into_iter().flatten().collect()
}
