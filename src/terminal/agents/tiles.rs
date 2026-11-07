//! The overview's groups as tiles: how they fit the screen (one tile size for all, groups side by
//! side where little room is wasted, panels packed in shelves) and how a group and a tile draw.
use super::panel::{self, Standing, Tone, on, standing};
use super::sections::totals;
use crate::protocol::AgentView;
use crate::protocol::views::{Section, SectionKind, ViewRow};
use crate::terminal::text::*;
use crate::terminal::{App, status, tools};
use ratatui::style::{Color, Stylize};
use ratatui::{buffer::Buffer, layout::Rect, text::Line, text::Span};

/// A tile's columns and rows, the rows its goal may take, and a blank row before its status; the
/// largest also lists its helpers by name, and from six rows a tile has the call running on a
/// line of its own. From three rows it ends in a foot of bars.
#[derive(Clone, Copy, Default)]
pub struct Size {
    pub w: usize,
    h: usize,
    goal: usize,
    gap: usize,
}
/// The sizes, largest first, as columns, rows, goal rows and gap.
const SIZES: [[usize; 4]; 5] = [
    [42, 9, 2, 1],
    [34, 6, 1, 0],
    [30, 4, 1, 0],
    [26, 2, 0, 0],
    [24, 1, 0, 0],
];
fn size([w, h, goal, gap]: [usize; 4]) -> Size {
    Size { w, h, goal, gap }
}

/// A panel's roots, most urgent first, each with the layout's rows after it.
pub type Roots<'a> = Vec<(&'a AgentView, &'a [ViewRow])>;
/// A top-level group as drawn: its tree, colours, whether folded, and its panels: its own agents
/// untitled, then each subgroup holding any, by name.
pub struct Group<'a> {
    pub tree: &'a [Section],
    pub tone: Tone,
    pub folded: bool,
    pub panels: Vec<(Option<&'a str>, Roots<'a>)>,
}
// One way to pack a group's panels: its height, each panel's columns of tiles, its shelves of
// panels side by side, and its widest shelf.
struct Packing {
    h: usize,
    cols: Vec<usize>,
    shelves: Vec<Vec<usize>>,
    wide: usize,
}
/// What a fit placed, in body rows from 0: each group's box and the row its content ends on,
/// each panel's box with its group and index, each tile's box with its group, agent and rows,
/// and the tile size.
#[derive(Default)]
pub struct Placed<'a> {
    pub groups: Vec<(Rect, u16)>,
    pub panels: Vec<(Rect, usize, usize)>,
    pub tiles: Vec<(Rect, usize, &'a AgentView, &'a [ViewRow])>,
    pub bottom: usize,
    waste: usize,
    pub size: Size,
}

fn rect(x: usize, y: usize, w: usize, h: usize) -> Rect {
    Rect::new(x as _, y as _, w as _, h as _)
}
// A panel `c` tiles of `tw` columns across and `r` tiles down, frame and margins included.
fn pw(c: usize, tw: usize) -> usize {
    4 + c * (tw + 1)
}
fn ph(r: usize, size: Size) -> usize {
    2 + r * size.h + (r - 1) * usize::from(size.h > 1)
}

/// The groups on a screen `w` columns wide with `room` rows under the header.
pub struct Overview<'a> {
    pub groups: Vec<Group<'a>>,
    pub w: usize,
    pub room: usize,
}
// Each group's ways to pack its panels at one size, and the one chosen of each.
type Options = [Vec<Packing>];
impl<'a> Overview<'a> {
    // The ways to pack `g`'s panels into its frame: one for each most rows a panel may take, from
    // one to the most agents any holds, as far as it changes the columns.
    fn packings(&self, g: &Group, size: Size) -> Vec<Packing> {
        let cw = self.w.saturating_sub(6);
        let most = (cw.saturating_sub(4) / (size.w + 1)).max(1);
        let largest = g.panels.iter().map(|p| p.1.len()).max().unwrap_or(1);
        let mut out: Vec<Packing> = Vec::new();
        for rows in 1..=largest {
            let n = |p: &(_, Roots)| most.min(p.1.len().div_ceil(rows));
            let cols: Vec<_> = g.panels.iter().map(n).collect();
            // Each panel's columns only fall as its rows grow, so a repeat repeats the last.
            if out.last().is_some_and(|p| p.cols == cols) {
                continue;
            }
            let (mut shelves, mut used, mut wide) = (vec![Vec::new()], 0, 0);
            for (i, c) in cols.iter().enumerate() {
                let w = pw(*c, size.w);
                if used > 0 && used + 2 + w > cw {
                    (used, _) = (0, shelves.push(Vec::new()));
                }
                used += w + 2 * usize::from(used > 0);
                wide = wide.max(used);
                shelves.last_mut().into_iter().for_each(|s| s.push(i));
            }
            let tall = |i: &usize| ph(g.panels[*i].1.len().div_ceil(cols[*i]), size);
            let shelf = |s: &Vec<usize>| s.iter().map(tall).max().unwrap_or(0) + 1;
            let h = shelves.iter().map(shelf).sum::<usize>() - 1;
            out.push(Packing {
                h,
                cols,
                shelves,
                wide,
            });
        }
        out
    }

    // Group `i`'s natural width and height in packing `p`, frame included; folded, a full line.
    fn natural(&self, i: usize, p: &Packing) -> (usize, usize) {
        [(p.wide + 4, p.h + 2), (self.w - 2, 1)][usize::from(self.groups[i].folded)]
    }

    // The groups' lines on the screen, each a run of them: side by side where they fit when
    // `side`, else one a line; group `i` packed in `at(i)`.
    fn lines<'p>(
        &self,
        at: impl Fn(usize) -> &'p Packing,
        side: bool,
    ) -> impl Iterator<Item = std::ops::Range<usize>> {
        let (n, full, mut start) = (self.groups.len(), self.w - 2, 0);
        std::iter::from_fn(move || {
            (start < n).then(|| {
                let (mut used, mut end) = (self.natural(start, at(start)).0, start + 1);
                while side && end < n && used + 2 + self.natural(end, at(end)).0 <= full {
                    (used, end) = (used + 2 + self.natural(end, at(end)).0, end + 1);
                }
                std::mem::replace(&mut start, end)..end
            })
        })
    }

    // The rows the groups take in the packings `pick` chose, group `step`'s one further on, a
    // blank row after each line.
    fn height(&self, opts: &Options, pick: &[usize], step: Option<usize>, side: bool) -> i64 {
        let at = |i: usize| &opts[i][pick[i] + usize::from(step == Some(i))];
        let tall = |l: std::ops::Range<usize>| l.map(|i| self.natural(i, at(i)).1).max();
        self.lines(at, side)
            .map(|l| tall(l).unwrap_or(0) as i64 + 1)
            .sum()
    }

    // The packings to use: each group's shortest, then a step at a time toward fewer, narrower
    // columns, the step that saves the most rows or costs the fewest first, while the whole
    // stays within the room or the step makes it shorter.
    fn choose(&self, opts: &Options, side: bool) -> Vec<usize> {
        let shortest = |o: &Vec<Packing>| (0..o.len()).min_by_key(|i| o[*i].h).unwrap_or(0);
        let mut pick: Vec<_> = opts.iter().map(shortest).collect();
        loop {
            let now = self.height(opts, &pick, None, side);
            let steps = (0..opts.len()).filter(|i| pick[*i] + 1 < opts[*i].len());
            let costs = steps.filter_map(|i| {
                let t = self.height(opts, &pick, Some(i), side);
                (t <= self.room as i64 + 1 || t < now).then_some((t - now, i))
            });
            match costs.min() {
                Some((_, i)) => pick[i] += 1,
                None => return pick,
            }
        }
    }

    // Places the groups at `size` in their packings: boxes, panels and tiles, each line of
    // groups stretched across the width and tiles widening to share the room left on a shelf.
    fn place(&self, opts: &Options, size: Size, side: bool) -> Placed<'a> {
        let pick = self.choose(opts, side);
        let at = |i: usize| &opts[i][pick[i]];
        let (full, mut y) = (self.w - 2, 0);
        let mut out = Placed {
            size,
            ..Placed::default()
        };
        for line in self.lines(at, side) {
            let natural = |i: usize| self.natural(i, at(i));
            let sum = line.clone().map(|i| natural(i).0).sum::<usize>().max(1);
            let spare = full.saturating_sub(sum + 2 * (line.len() - 1));
            let tall = line.clone().map(|i| natural(i).1).max().unwrap_or(0);
            let mut x = 1;
            for i in line.clone() {
                let (nw, nh) = natural(i);
                let gw = [nw + spare * nw / sum, 1 + full - x][usize::from(i + 1 == line.end)];
                out.waste += (tall - nh) * gw;
                let end = self.shelves(&mut out, (i, at(i)), (x, y + 1), gw, size);
                out.groups
                    .push((rect(x, y, gw, [nh, tall][usize::from(side)]), end));
                x += gw + 2;
            }
            y += tall + 1;
        }
        out.bottom = y.saturating_sub(1);
        out
    }

    // Places group `i`'s panels and tiles in its shelves from `x, y` in a frame `gw` wide;
    // returns the row its content ends on.
    fn shelves(
        &self,
        out: &mut Placed<'a>,
        (i, p): (usize, &Packing),
        (x, mut y): (usize, usize),
        gw: usize,
        size: Size,
    ) -> u16 {
        let g = &self.groups[i];
        for shelf in p.shelves.iter().filter(|_| !g.folded) {
            let base: usize = shelf.iter().map(|j| pw(p.cols[*j], size.w) + 2).sum();
            let free = (gw + 2).saturating_sub(base + 4);
            let cols: usize = shelf.iter().map(|j| p.cols[*j]).sum();
            let tw = size.w + (free / cols).min(size.w);
            let (mut px, mut high) = (x + 2, 0);
            for &j in shelf {
                let (c, agents) = (p.cols[j], &g.panels[j].1);
                let (w, h) = (pw(c, tw), ph(agents.len().div_ceil(c), size));
                out.panels.push((rect(px, y, w, h), i, j));
                let step = size.h + usize::from(size.h > 1);
                for (n, (a, below)) in agents.iter().enumerate() {
                    let (tx, ty) = (px + 3 + n % c * (tw + 1), y + 1 + n / c * step);
                    out.tiles.push((rect(tx, ty, tw, size.h), i, a, below));
                }
                (px, high) = (px + w + 2, high.max(h));
            }
            y += high + 1;
        }
        y as u16 - 1
    }

    /// The largest tile size whose layout fits, groups side by side where that wastes under 8
    /// percent of the screen, its tiles grown by up to four rows where the agents' words need
    /// them; else the smallest, which scrolls. A size fits only where one tile's panel fits in
    /// a group's frame across the screen; a screen too narrow for the smallest narrows it.
    pub fn fit(&self) -> Placed<'a> {
        let (fits, cw) = (|p: &Placed| p.bottom <= self.room, self.w.saturating_sub(6));
        let across = |s: &Size| pw(1, s.w) <= cw && (self.w >= 100 || s.w < 42);
        let mut smallest = size(SIZES[4]);
        smallest.w = smallest.w.min(cw.saturating_sub(5));
        let sizes = SIZES[..4].iter().map(|s| size(*s)).filter(across);
        let options =
            |size| -> Vec<_> { self.groups.iter().map(|g| self.packings(g, size)).collect() };
        let mut last = Placed::default();
        for size in sizes.chain([smallest]) {
            let opts = options(size);
            let beside = self.place(&opts, size, true);
            let stack = self.place(&opts, size, false);
            let small = beside.waste * 100 < 8 * self.w * (self.room + 6);
            let side = fits(&beside) && (!fits(&stack) || small);
            last = if side { beside } else { stack };
            if !fits(&last) {
                continue;
            }
            let most = self.need(size).saturating_sub(size.h).min(4) * usize::from(size.h > 2);
            for extra in (1..=most).rev() {
                let mut size = size;
                size.h += extra;
                let grown = self.place(&options(size), size, side);
                if fits(&grown) {
                    return grown;
                }
            }
            break;
        }
        last
    }

    // The most rows any agent's words would fill in a tile of `size` widened by three fifths.
    fn need(&self, size: Size) -> usize {
        let iw = (size.w * 8).div_ceil(5) - 3;
        let rows = |(a, below): &(&AgentView, &[ViewRow])| {
            let s = standing(a);
            let words = wrapped(&format!("↳ {}", s.said(a).2), iw, 6);
            let helpers = size.gap > 0 && below.first().is_some_and(|r| r.depth > 0);
            let step = usize::from(size.h >= 6 && stepped(a, s)) + usize::from(helpers);
            2 + wrapped(goal(a), iw, size.goal) + size.gap + words + step
        };
        let all = self
            .groups
            .iter()
            .flat_map(|g| &g.panels)
            .flat_map(|p| &p.1);
        all.map(rows).max().unwrap_or(0)
    }
}

// Rows `words` wraps to at `room` columns, at most `most`.
fn wrapped(words: &str, room: usize, most: usize) -> usize {
    let mut rows = 0;
    visit_wrap(&[Span::raw(words)], room, |parts, _, last| {
        rows += usize::from(!last || !parts.is_empty());
        rows < most
    });
    rows
}

// Whether a tile shows the call running now on a line of its own: while working with words of
// its own to say first (with none, the call is what it says).
fn stepped(a: &AgentView, s: Standing) -> bool {
    s == Standing::Working && a.doing.is_some() && !a.last_reply.is_empty()
}

// An agent's goal: its headline, else its name.
fn goal(a: &AgentView) -> &str {
    [&a.headline, &a.name][usize::from(a.headline.is_empty())]
}

/// Draws the frame round `r` in `ink` on `ground`, its top ticked, with its index on its foot in
/// readable ink.
pub fn frame(
    b: &mut Buffer,
    r: Rect,
    (ink, ground): (Color, Color),
    ticks: bool,
    index: Option<(usize, Color)>,
) {
    let (w, line) = (r.width as usize, style(ink, ground));
    put(b, r.x, r.y, &panel::top(w, ticks), r.width, line);
    for y in r.y + 1..r.bottom().saturating_sub(1) {
        put(b, r.x, y, "│", 1, line);
        put(b, r.right() - 1, y, "│", 1, line);
    }
    let (index, lit) = index.map_or((None, ink), |(n, lit)| (Some(n), lit));
    let foot = panel::foot(w, ticks, index, [ink, lit, ground]);
    panel::write(b, r.x, r.bottom() - 1, &foot);
}

/// Group `g` in `r`: its frame on its ground with the number and name tabs, how many need the
/// person, its counts at the right, and the hatch over the room under its content past `end`;
/// folded, one line with its census, marked `❯` while `lit`. It is number `i + 1`, as NEEDS YOU,
/// drawn first, has none.
pub fn group(app: &App, b: &mut Buffer, (g, i, lit): (&Group, usize, bool), (r, end): (Rect, u16)) {
    let n = (g.tree[0].kind != SectionKind::NeedsYou).then_some(i + 1);
    let (tone, w) = (g.tone, r.width as usize);
    let roots = panel::rows(g.tree).filter(|r| r.depth == 0);
    let census = panel::census(roots.filter_map(|r| app.agents.get(&r.session)));
    let ((agents, helpers), needs) = (totals(g.tree), census[0] + census[1]);
    let mut tags = panel::named(n, &g.tree[0].name, tone);
    let you = [" ◆ {} ", " ◆ {} NEEDS YOU ", " ◆ {} NEED YOU "];
    let you = you[needs.min(2) * usize::from(w > 50)].replace("{}", &needs.to_string());
    let you = on(you, AMBER, tone.ground, true);
    tags.extend((needs > 0).then_some(you));
    fill(b, r, style(DATA, tone.ground));
    if g.folded {
        let ink = style(mix(tone.ground, tone.hue, 0.5), tone.ground);
        put(b, r.x, r.y, &panel::shut(w), r.width, ink);
        if lit {
            put(b, r.x, r.y, "❯", 1, bold(CYAN, tone.ground));
        }
        let census = [0, census[1], census[2], census[3]];
        tags.extend(panel::counts(census, tone.ground));
    } else {
        let index = n.map(|n| (n, tone.soft));
        frame(b, r, (tone.frame, tone.ground), true, index);
        let hatch = style(mix(tone.ground, tone.hue, 0.28), tone.ground);
        for y in end + 1..r.bottom().saturating_sub(1) {
            let xs = (r.x + 2..r.right().saturating_sub(2)).filter(|x| (x + y) % 7 == 0);
            xs.for_each(|x| _ = put(b, x, y, "╱", 1, hatch));
        }
    }
    let fold = [" ", " ▸ "][usize::from(g.folded)];
    let agents = panel::count(agents, "AGENT");
    let more = panel::count(helpers, "HELPER");
    let words = [
        format!(" {agents} · {more}{fold}"),
        format!(" {agents}{fold}"),
    ];
    // The tabs give way to the shortest counts, so neither crosses the frame.
    let tags = clip_spans(tags, w.saturating_sub(6 + width(&words[1])));
    let x = panel::write(b, r.x + 2, r.y, &tags);
    let room = r.right().saturating_sub(x + 4) as usize;
    let mut words = words.into_iter().skip(usize::from(helpers == 0));
    if let Some(words) = words.find(|s| width(s) <= room) {
        let at = r.right() - 3 + u16::from(g.folded) - width(&words) as u16;
        panel::write(b, at, r.y, &[on(words, tone.soft, tone.ground, false)]);
    }
}

/// A panel in `r`: its frame, its subgroup's name and its census by state on its top edge.
pub fn shelf(b: &mut Buffer, r: Rect, tone: Tone, (name, roots): &(Option<&str>, Roots)) {
    let ink = mix(tone.ground, tone.hue, 0.3);
    frame(b, r, (ink, tone.ground), false, None);
    let title = name
        .map(|n| format!(" ▸ {} ", n.to_uppercase()))
        .unwrap_or_default();
    let title = on(title, mix(tone.hue, WHITE, 0.45), tone.ground, true);
    let census = panel::counts(panel::census(roots.iter().map(|a| a.0)), tone.ground);
    let room = (r.width as usize).saturating_sub(4 + spans_width(&census));
    panel::write(b, r.x + 2, r.y, &clip_spans(vec![title], room));
    let at = r.right().saturating_sub(1 + spans_width(&census) as u16);
    panel::write(b, at, r.y, &census);
}

/// One agent's tile at `size` in `r`: its edge, its name row (solid cyan while `lit`), its goal,
/// what it is doing, the call running now, its helpers by name in the largest tile, and a foot
/// of the hour's bars, the hour's count and where it runs.
pub fn tile(
    app: &App,
    b: &mut Buffer,
    r: Rect,
    agent: (&AgentView, &[ViewRow]),
    (tone, size): (Tone, Size),
    lit: bool,
    now: u64,
) {
    let ((a, below), s) = (agent, standing(agent.0));
    let (ground, edge) = panel::ground(app, s, tone, now);
    fill(b, r, style(DATA, ground));
    (r.y..r.bottom()).for_each(|y| _ = put(b, r.x, y, "▌", 1, style(edge, ground)));
    let (x, w, last) = (r.x + 2, r.width.saturating_sub(3) as usize, r.bottom() - 1);
    let helpers = below.iter().take_while(|r| r.depth > 0);
    let quiet = mix(mix(DATA, PANEL_INK, 0.25), tone.hue, 0.25);
    let aim = on(goal(a).to_owned(), quiet, ground, false).italic();
    let (lead, lead_ink, words, ink) = s.said(a);
    let lead = on(format!("{lead} "), lead_ink, ground, false);
    let said = vec![lead, Span::styled(words, ink.bg(ground))];
    let short = [vec![aim.clone()], said.clone()][usize::from(s.needs())].clone();
    let marks = panel::marks(app, helpers.clone(), now);
    let gap = on("  ", DATA, ground, false);
    let after = [Vec::new(), [vec![gap], short.clone()].concat()][usize::from(size.h == 1)].clone();
    let look = (ground, lit);
    let head = panel::head(app, a, (marks, after), size.h > 1, w + 1, look, now);
    panel::write(b, r.x + 1, r.y, &head);
    if size.h <= 2 {
        let line = clip_spans(short, w * usize::from(size.h == 2));
        panel::write(b, x, r.y + 1, &line);
        return;
    }
    let mut lines: Vec<Line> = wrap(vec![aim], w).into_iter().take(size.goal).collect();
    lines.extend((0..size.gap).map(|_| Line::default()));
    let step = a.doing.as_ref().filter(|_| size.h >= 6 && stepped(a, s));
    let names = helpers.filter_map(|r| app.agents.get(&r.session)).map(|h| {
        let (mark, word) = panel::helper(app, h, now);
        let ink = [PANEL_GRAY, AMBER][usize::from(word.starts_with("waiting"))];
        [mark, on(format!(" {}  ", h.name), ink, ground, false)]
    });
    let names: Vec<_> = names.take_while(|_| size.gap > 0).flatten().collect();
    let taken = lines.len() + usize::from(step.is_some()) + usize::from(!names.is_empty());
    let most = (size.h - 2).saturating_sub(taken).max(1);
    let mut status = wrap(said, w);
    if status.len() > most {
        status.truncate(most);
        status[most - 1].spans.push(on(" …", DATA, ground, false));
    }
    lines.extend(status);
    if let Some((tool, target, at)) = step {
        let mark = tools::mark(tool, &serde_json::Value::Null);
        let words = format!("{mark} {target} · {}", status::ago(app, *at, now));
        lines.push(on(words, PANEL_GRAY, ground, false).into());
    }
    lines.extend((!names.is_empty()).then(|| Line::from(names)));
    for (y, l) in (r.y + 1..last).zip(lines) {
        panel::write(b, x, y, &clip_spans(l.spans, w));
    }
    let (bars, calls) = panel::bars(app, a, 12, tone.hue, now);
    let x = panel::write(b, x, last, &bars) + 1;
    let hot = on(calls.to_string(), mix(tone.hue, WHITE, 0.35), ground, true);
    panel::write(b, x, last, &[on("1H ", PANEL_GRAY, ground, false), hot]);
    let server = status::server(app, &a.session).map_or("", |s| s.label.as_str());
    let place = format!("{server} · {}", a.project);
    let place = on(place, mix(PANEL_GRAY, tone.hue, 0.25), ground, false);
    if w > 22 + width(&place.content) {
        panel::write(b, r.right() - width(&place.content) as u16, last, &[place]);
    }
}
