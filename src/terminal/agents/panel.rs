//! The instrument panel the overview and the list share: a group's colours, frames with their
//! ticks, tabs, hatch and index, how an agent stands and what it says, the census by state, the
//! last hour's bars and the Fine clock.
use crate::protocol::views::{Colour, Section, ViewRow};
use crate::protocol::{AgentView, State};
use crate::terminal::{App, cards, motion, status, text::*, tools, topbar};
use ratatui::style::{Color, Style};
use ratatui::{buffer::Buffer, text::Span};

pub type Spans = Vec<Span<'static>>;

/// A group's colours on the panel: its panel form, its ground (13 percent of it over the panel),
/// its frame (half way from the ground) and its light ink for counts.
#[derive(Clone, Copy)]
pub struct Tone {
    pub hue: Color,
    pub ground: Color,
    pub frame: Color,
    pub soft: Color,
}
pub fn tone(colour: Option<Colour>) -> Tone {
    let hue = match colour {
        Some(Colour::Steel) => PANEL_STEEL,
        Some(Colour::Indigo) => PANEL_INDIGO,
        Some(Colour::Plum) => PANEL_PLUM,
        Some(Colour::Rose) => PANEL_ROSE,
        Some(Colour::Clay) => PANEL_CLAY,
        Some(Colour::Sand) => PANEL_SAND,
        Some(Colour::Moss) => PANEL_MOSS,
        Some(Colour::Slate) | None => PANEL_SLATE,
    };
    let ground = mix(PANEL, hue, 0.13);
    let (frame, soft) = (mix(ground, hue, 0.5), mix(hue, WHITE, 0.25));
    Tone {
        hue,
        ground,
        frame,
        soft,
    }
}

/// How the panel shows an agent, in the order it is listed: asking, its turn failed, stuck,
/// working, idle.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standing {
    Asks,
    Error,
    Stuck,
    Working,
    Idle,
}
pub fn standing(a: &AgentView) -> Standing {
    match a.state {
        _ if !a.asks.is_empty() => Standing::Asks,
        State::Stuck => Standing::Stuck,
        State::Working => Standing::Working,
        // Only an agent waiting for an input stands failed: a done one's last error is behind it.
        State::NeedsYou if a.error.is_some() => Standing::Error,
        _ => Standing::Idle,
    }
}
/// Each census column's mark and ink: asking, stuck or failed, working, idle.
pub const CENSUS: [(&str, Color); 4] = [("◆", AMBER), ("▲", RED), ("●", GREEN), ("○", PANEL_GRAY)];
impl Standing {
    /// Its census column.
    pub fn column(self) -> usize {
        [0, 1, 1, 2, 3][self as usize]
    }
    pub fn mark(self) -> (&'static str, Color) {
        CENSUS[self.column()]
    }
    /// Whether it waits on the person.
    pub fn needs(self) -> bool {
        self < Standing::Working
    }
    /// The solid chip at the right of its name row.
    pub fn chip(self) -> Option<&'static str> {
        [Some(" ASKS "), Some(" ERROR "), Some(" STUCK "), None, None][self as usize]
    }
    /// What it says now as its row draws it: the lead mark and its ink, the words and their
    /// style. The question, the error, why it is stuck, its latest words while working, or its
    /// latest reply dimmed while idle (with none, its headline, then "No reply yet").
    pub fn said(self, a: &AgentView) -> (&'static str, Color, String, Style) {
        let light = |ink| Style::new().fg(mix(ink, WHITE, 0.45));
        let reply = [&a.last_reply, &a.headline]
            .into_iter()
            .find(|s| !s.is_empty());
        match self {
            Standing::Asks => ("↳", AMBER, a.asks[0].text.clone(), light(AMBER)),
            Standing::Error | Standing::Stuck => {
                let why = a.error.as_deref().unwrap_or(cards::STUCK);
                ("↳", RED, why.to_owned(), light(RED))
            }
            Standing::Working => {
                let mark = |tool: &str| tools::mark(tool, &serde_json::Value::Null);
                let words = (a.doing.as_ref()).map_or("thinking".into(), |(tool, target, _)| {
                    format!("{} {target}", mark(tool))
                });
                let words = [&a.last_reply, &words][usize::from(a.last_reply.is_empty())];
                let closing = (a.closing).then(|| "closing after this turn".to_owned());
                (
                    "↳",
                    VIOLET,
                    closing.unwrap_or(words.clone()),
                    Style::new().fg(DATA),
                )
            }
            Standing::Idle => {
                let words = reply.map_or("No reply yet".into(), String::clone);
                ("↺", GRAY_DIM, words, Style::new().fg(PANEL_GRAY).italic())
            }
        }
    }
}

/// Agents counted by census column.
pub fn census<'a>(agents: impl Iterator<Item = &'a AgentView>) -> [usize; 4] {
    let mut n = [0; 4];
    agents.for_each(|a| n[standing(a).column()] += 1);
    n
}
/// The counts that are not zero as `◆1 ●3 ○2`, each in its state's ink, on `ground`.
pub fn counts(n: [usize; 4], ground: Color) -> Spans {
    let marks = (n.into_iter().zip(CENSUS)).filter(|(n, _)| *n > 0);
    marks
        .map(|(n, (mark, ink))| Span::styled(format!("{mark}{n} "), style(ink, ground)))
        .collect()
}
/// One bar `cells` wide split among the states' inks by `n`.
pub fn bar(n: [usize; 4], cells: usize, ground: Color) -> Spans {
    let (total, mut done, mut acc) = (n.iter().sum::<usize>().max(1), 0, 0);
    let mut out = Vec::new();
    for (count, (_, ink)) in n.into_iter().zip(CENSUS) {
        acc += count;
        let to = (acc * cells + total / 2) / total;
        out.push(Span::styled("▆".repeat(to - done), style(ink, ground)));
        done = to;
    }
    out
}

/// A frame's top edge `w` columns wide, a tick every five columns when `ticks`.
pub fn top(w: usize, ticks: bool) -> String {
    let at = |i| match i {
        0 => '╭',
        _ if i + 1 == w => '╮',
        _ if ticks && i % 5 == 0 => '┴',
        _ => '─',
    };
    (0..w).map(at).collect()
}
/// A folded frame's one line: its top edge with its corners open.
pub fn shut(w: usize) -> String {
    let inner: String = top(w, true)
        .chars()
        .skip(1)
        .take(w.saturating_sub(2))
        .collect();
    format!("╶{inner}╴")
}
/// A frame's foot `w` columns wide in `ink` on `ground`; a ticked frame's, from 25 columns, has a
/// hatch at its left and its index at its right, the number in readable `lit`.
pub fn foot(w: usize, ticked: bool, index: Option<usize>, [ink, lit, ground]: [Color; 3]) -> Spans {
    let ticked = ticked && w > 24;
    let at = |i| match i {
        0 => '╰',
        _ if i + 1 == w => '╯',
        2..8 if ticked => '╱',
        _ => '─',
    };
    let edge: String = (0..w).map(at).collect();
    let n = index
        .filter(|_| ticked)
        .map(|n| on(format!("{n:02}"), lit, ground, false));
    let n = n.map(|n| {
        (
            w - 5 - width(&n.content),
            vec![on("··· ", ink, ground, false), n],
        )
    });
    over(&edge, ink, ground, n.into_iter().collect())
}
/// `edge` in `ink` on `ground` with each run of `parts` laid over it from its column; a run that
/// would start inside the one before starts after it.
pub fn over(edge: &str, ink: Color, ground: Color, parts: Vec<(usize, Spans)>) -> Spans {
    let chars: Vec<char> = edge.chars().collect();
    let (mut out, mut x) = (Vec::new(), 0);
    let base = |from: usize, to: usize| {
        let s: String = chars[from.min(chars.len())..to.min(chars.len())]
            .iter()
            .collect();
        Span::styled(s, style(ink, ground))
    };
    for (at, spans) in parts {
        let at = at.max(x);
        out.push(base(x, at));
        x = at + spans_width(&spans);
        out.extend(spans);
    }
    out.push(base(x, chars.len()));
    out
}
/// Solid chips, bold, ending in a slant in the last chip's colour over `ground`.
pub fn tabs(chips: &[(&str, Color, Color)], ground: Color) -> Spans {
    let mut out: Spans = chips
        .iter()
        .map(|(t, ink, bg)| on(t.to_string(), *ink, *bg, true))
        .collect();
    out.push(on("◤", chips.last().map_or(ground, |c| c.2), ground, false));
    out
}
/// A group's number and name tabs.
pub fn named(n: Option<usize>, name: &str, tone: Tone) -> Spans {
    let (number, name) = (
        n.map(|n| format!(" {n} ")),
        format!(" {} ", name.to_uppercase()),
    );
    let number = number
        .as_deref()
        .map(|n| (n, CHIP_INK, mix(tone.hue, WHITE, 0.5)));
    let chips: Vec<_> = number
        .into_iter()
        .chain([(name.as_str(), CHIP_INK, tone.hue)])
        .collect();
    tabs(&chips, tone.ground)
}
/// Words in `fg` on `bg`, bold or not.
pub fn on(
    text: impl Into<std::borrow::Cow<'static, str>>,
    fg: Color,
    bg: Color,
    strong: bool,
) -> Span<'static> {
    Span::styled(text, [style(fg, bg), bold(fg, bg)][usize::from(strong)])
}
/// `8 AGENTS`, `1 AGENT`.
pub fn count(n: usize, word: &str) -> String {
    format!("{n} {word}{}", plural(n).to_uppercase())
}
/// The columns `spans` take.
/// Writes `spans` from `x, y` within the buffer; returns the column after them.
pub fn write(b: &mut Buffer, x: u16, y: u16, spans: &[Span]) -> u16 {
    let mut at = x;
    for s in spans {
        let room = b.area.right().saturating_sub(at);
        at = put(b, at, y, &s.content, room, s.style);
    }
    x + spans_width(spans) as u16
}
/// `left` and `right` in `w` columns, `left` clipped first and the gap between them on `bg`.
pub fn ends(left: Spans, right: Spans, w: usize, bg: Color) -> Spans {
    let mut out = clip_spans(left, w.saturating_sub(spans_width(&right) + 1));
    let gap = " ".repeat(w.saturating_sub(spans_width(&out) + spans_width(&right)));
    out.push(on(gap, DATA, bg, false));
    out.extend(right);
    out
}
/// Every row of a tree's sections.
pub fn rows(tree: &[Section]) -> impl Iterator<Item = &ViewRow> {
    tree.iter().flat_map(|s| &s.sessions)
}
/// An agent's name row `w` columns: a space, its mark, its bold name and `after`, then its chip
/// (when `chip`) or its helpers' `marks`, and its age; on `ground`, or while `lit` a solid cyan
/// bar in dark ink with `▶` for its mark.
pub fn head(
    app: &App,
    a: &AgentView,
    (marks, after): (Spans, Spans),
    chip: bool,
    w: usize,
    (ground, lit): (Color, bool),
    now: u64,
) -> Spans {
    let s = standing(a);
    let bg = [ground, CYAN][usize::from(lit)];
    let dark = |c: Option<Color>| [c.unwrap_or(PANEL_GRAY), CHIP_INK][usize::from(lit)];
    let mut right: Spans = match s.chip().filter(|_| chip) {
        Some(c) => vec![on(c, CHIP_INK, [s.mark().1, WHITE][usize::from(lit)], true)],
        None => {
            let mark = |m: Span<'static>| Span::styled(m.content, style(dark(m.style.fg), bg));
            marks.into_iter().map(mark).collect()
        }
    };
    let age = format!(" {}", status::ago(app, a.last_activity, now));
    right.push(on(age, dark(None), bg, false));
    let (mark, ink) = [s.mark(), ("▶", CHIP_INK)][usize::from(lit)];
    let name = on(a.name.clone(), dark(Some(WHITE)), bg, true);
    let mut left = vec![on(format!(" {mark} "), ink, bg, true), name];
    let quiet = mix(CYAN, CHIP_INK, 0.75);
    let after = after.into_iter().map(|t| {
        let fg = t.style.fg.filter(|_| !lit).unwrap_or(quiet);
        Span::styled(t.content, t.style.bg(bg).fg(fg))
    });
    // Words that would show fewer than six columns are left out rather than cut to a stub.
    let room = w.saturating_sub(spans_width(&left) + spans_width(&right) + 1);
    left.extend(after.filter(|_| room >= 8));
    ends(left, right, w, bg)
}
/// Bar heights from a blank to a full cell.
const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// The newest `n` of the hour's twelve five-minute bars as of `now`, in `hue` with the tallest
/// lit toward white, and the hour's count. Without the fact the bars are empty. Heights are
/// against the busiest period, or sixteen calls when none is busier; the bars move on at the
/// next five-minute step, which this asks a frame for.
pub fn bars(app: &App, a: &AgentView, n: usize, hue: Color, now: u64) -> (Spans, u32) {
    let calls = a.hour.map_or([0; 12], |h| h.at(now).calls);
    app.next.every(now, 300_000);
    let most = u32::from(*calls.iter().max().unwrap_or(&0)).max(16);
    let spans = calls[12 - n.min(12)..].iter().map(|c| {
        let level = (u32::from(*c) * 8).div_ceil(most) as usize;
        let ink = [hue, mix(hue, WHITE, 0.35)][usize::from(level >= 6)];
        Span::styled(EIGHTHS[level], Style::new().fg(ink))
    });
    (spans.collect(), calls.iter().map(|c| u32::from(*c)).sum())
}

/// The Fine clock's digits and colon: a 5 × 7 pixel font, a blank pixel row above, as two rows of
/// octants; a digit is three columns and the colon one, as in the top bar's half-block font.
const FINE: [[&str; 2]; 11] = [
    ["𜵋𜵼▖", "𜶍𜶬𜴍"],
    ["𜴘𜵈 ", "𜺠𜷀 "],
    ["𜴐𜴆▖", "𜷋𜶱𜺣"],
    ["𜴆𜵝𜴀", "𜶀𜶭🯦"],
    ["𜺠𜶤 ", "𜴇𜶛𜴀"],
    ["𜷇𜶷𜴀", "𜶀▂𜴍"],
    ["𜵑𜴆 ", "𜶇𜶮🯦"],
    ["𜴆𜵼🯦", "𜶖𜺨 "],
    ["𜵋𜴆▖", "𜶆𜶮🯦"],
    ["𜵋𜴆▖", "𜵲𜵔▘"],
    ["𜺣", "𜴀"],
];
/// Draws the time of day at `x, y` in the Fine clock, or in half blocks without octants, in the
/// clock's cyan; returns the column after it. Asks a frame at the next minute.
pub fn clock(app: &App, b: &mut Buffer, x: u16, y: u16, now: u64) -> u16 {
    let time = zoned(now, app.zone.as_ref()).map(|t| t.strftime("%-I:%M").to_string());
    app.next.every(now, 60_000);
    let mut end = x;
    for row in 0..2 {
        end = x;
        for c in time.as_deref().unwrap_or_default().chars() {
            let glyph = match (c.to_digit(10), app.octant()) {
                (Some(d), true) => FINE[d as usize][row],
                (None, true) => FINE[10][row],
                (Some(d), false) => topbar::HALF[d as usize][row],
                (None, false) => ["▄", "▀"][row],
            };
            end = put(b, end, y + row as u16, glyph, 3, style(CLOCK, PANEL));
        }
    }
    end
}
/// The seconds while motion plays, asking a frame each second; with motion still, none.
pub fn seconds(app: &App, now: u64) -> Option<String> {
    let t = zoned(now, app.zone.as_ref()).filter(|_| motion::on(app))?;
    app.next.every(now, 1000);
    Some(t.strftime("%S").to_string())
}

/// A tile's or row's ground for an agent that stands `s` in a group of `tone`, and its left
/// edge's ink. With motion on, a question's ground breathes toward amber over 2.4 s and a working
/// edge toward its ground over 1.6 s, sampled at the helper pulse's step.
pub fn ground(app: &App, s: Standing, tone: Tone, now: u64) -> (Color, Color) {
    let wave = |period: f64| {
        let on = motion::on(app) && matches!(s, Standing::Asks | Standing::Working);
        if on {
            app.next.every(now, cards::PULSE.0);
        }
        let phase = (now % period as u64) as f64 / period * std::f64::consts::TAU;
        [0., 0.5 - 0.5 * phase.cos()][usize::from(on)]
    };
    let ground = match s {
        Standing::Asks => mix(AMBER_BG, mix(AMBER_BG, AMBER, 0.08), wave(2400.)),
        Standing::Error | Standing::Stuck => RED_BG,
        Standing::Working => mix(tone.ground, tone.hue, 0.11),
        Standing::Idle => mix(tone.ground, tone.hue, 0.05),
    };
    let edge = match s {
        Standing::Working => mix(GREEN, ground, 0.55 * wave(1600.)),
        Standing::Idle => mix(ground, tone.hue, 0.45),
        _ => s.mark().1,
    };
    (ground, edge)
}

/// A helper's mark and its state in words, read from how it stands as its own row reads it:
/// turning in the green every working agent wears while it works, `◆` waiting on the person or
/// for its merge, `▲` stuck or failed, else its card's glyph and word.
pub fn helper(app: &App, a: &AgentView, now: u64) -> (Span<'static>, &'static str) {
    let card = cards::Card::of(app, a, now);
    let (mark, ink, word) = match (standing(a), card.word) {
        (Standing::Asks, _) => ("◆", AMBER, "waiting on you"),
        (Standing::Error, _) => ("▲", RED, "error"),
        (Standing::Stuck, _) => ("▲", RED, "stuck"),
        // Still, the pulse's first frame: a helper never wears its parent's `●`.
        (Standing::Working, _) if card.glyph == "●" => (cards::PULSE.1[0], GREEN, "working"),
        (Standing::Working, _) => (card.glyph, GREEN, "working"),
        (_, "awaiting merge") => ("◆", AMBER, "waiting for merge"),
        (_, word) => (card.glyph, PANEL_GRAY, word),
    };
    (Span::styled(mark, Style::new().fg(ink)), word)
}

/// A root's helpers' marks for its name row, from the layout's rows under it: each listed
/// helper's, a space after each, then `✓N` for its finished ones.
pub fn marks<'a>(app: &App, below: impl Iterator<Item = &'a ViewRow>, now: u64) -> Spans {
    let (mut out, mut done) = (Vec::new(), 0);
    for (r, h) in below.filter_map(|r| Some((r, app.agent_row(&r.session)?))) {
        let (mark, word) = helper(app, h, now);
        match r.folded || word == "finished" {
            true => done += 1,
            false => out.extend([mark, Span::raw(" ")]),
        }
    }
    if done > 0 {
        out.push(Span::styled(format!("✓{done} "), PANEL_GRAY));
    }
    out
}
