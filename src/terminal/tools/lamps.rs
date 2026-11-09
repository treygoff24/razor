//! Skill lamps in the band.
use super::skill::{DIM, HI};
use super::*;
use crate::protocol::SkillState;
use crate::terminal::{
    App,
    transcript::Transcript,
    view::{Action, Hit},
};
use ratatui::style::Style;

/// The server's skill slot, whose card Alt and its digit open.
pub(in crate::terminal) fn slot_entry(t: &Transcript, slot: usize) -> Option<EntryId> {
    t.skills.get(slot).map(|s| s.entry)
}

/// Reserve the lamp band whenever the server lists skills.
pub(in crate::terminal) fn rows(t: Option<&Transcript>) -> u16 {
    2 * u16::from(t.is_some_and(|t| !t.skills.is_empty()))
}

fn icon<'a>(t: &'a Transcript, state: &SkillState) -> &'a str {
    t.entries
        .get(&state.entry)
        .and_then(|e| t.call(e, None).decoded)
        .and_then(|d| d.skill.as_ref())
        .map_or("🧩", |s| s.icon.as_str())
}

/// Draws only the focused session's lamps; asks for a 50 ms frame only while a visible lamp plays.
/// Under 56 columns and with motion off, the lamps are settled and ask for no frames.
pub(in crate::terminal) fn draw(
    app: &App,
    t: &Transcript,
    b: &mut Buffer,
    r: Rect,
    now: u64,
    hits: &mut Vec<Hit>,
) {
    if r.height < 2 {
        return;
    }
    let chips = &t.skills;
    if chips.is_empty() {
        return;
    }
    fill(b, r, style(DATA, RECESS));
    if r.width < 56 {
        return compact(b, r, chips);
    }
    let widths: Vec<_> = chips
        .iter()
        .enumerate()
        .map(|(slot_id, c)| {
            let icon = if c.worn.is_some() {
                0
            } else {
                width(icon(t, c)) + 1
            };
            width(&slot(slot_id)) + 3 + icon + width(&c.name)
        })
        .collect();
    let shown = fit(&widths, r.width.saturating_sub(2) as usize);
    let mut x = r.x + 1;
    // A click on a lamp opens or closes its skill's card.
    for (i, chip) in chips.iter().take(shown).enumerate() {
        if i > 0 {
            x += 3;
        }
        let start = x;
        x = draw_chip(app, t, b, r, x, (i, chip), now);
        if let Some(id) = app.focus.clone() {
            let area = Rect::new(start, r.y, x.saturating_sub(start), 2);
            hits.push((area, Action::Call(id, chip.entry)));
        }
    }
    // The lamps that do not fit, counted.
    if shown < chips.len() {
        let more = format!(" +{}", chips.len() - shown);
        lamp(b, r, x, &more, style(GRAY, RECESS));
    }
}

/// `words` on the lamps' second row from `x` to the right edge; returns the next column.
fn lamp(b: &mut Buffer, r: Rect, x: u16, words: &str, ink: Style) -> u16 {
    put(b, x, r.y + 1, words, r.right().saturating_sub(x), ink)
}

fn slot(i: usize) -> String {
    if i < 9 {
        (i + 1).to_string()
    } else {
        " ".into()
    }
}

fn compact(b: &mut Buffer, r: Rect, chips: &[SkillState]) {
    let active = chips.iter().filter(|c| c.worn.is_none()).count();
    let count = format!(" ⚡{active}  ");
    let mut x = lamp(b, r, r.x, &count, bold(LAMP, RECESS));
    for c in chips {
        if x + 2 >= r.right() {
            break;
        }
        let (glyph, ink) = if c.worn.is_none() {
            ("◉", LAMP)
        } else {
            ("○", GRAY_DIM)
        };
        x = lamp(b, r, x, glyph, style(ink, RECESS));
    }
    lamp(b, r, x, "  ▸", style(GRAY, RECESS));
}

fn fit(widths: &[usize], room: usize) -> usize {
    let mut used = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 3;
    let mut n = widths.len();
    if used <= room {
        return n;
    }
    while n > 0 && used + width(&format!(" +{}", widths.len() - n)) > room {
        n -= 1;
        used = used.saturating_sub(widths[n] + usize::from(n > 0) * 3);
    }
    n
}

fn draw_chip(
    app: &App,
    t: &Transcript,
    b: &mut Buffer,
    r: Rect,
    x: u16,
    (slot_id, chip): (usize, &SkillState),
    now: u64,
) -> u16 {
    let motion = crate::terminal::motion::on(app);
    let arriving = skill::burst(now, chip.at, 1100, motion);
    let wearing = t
        .worn_skill(&chip.name)
        .and_then(|at| skill::burst(now, at, 900, motion));
    let age = wearing.or(arriving);
    let off = wearing.is_some();
    if age.is_some() {
        app.next.at(now.saturating_add(50));
    }
    let (glyph, ink, glow) = phase(age, off, chip.worn.is_some());
    let ground = mix(RECESS, rgb(0x3a2d0f), glow);
    let active = chip.worn.is_none() || off;
    let slot_ink = if active { GRAY } else { GRAY_DIM };
    let mut at = lamp(
        b,
        r,
        x,
        &format!("{} ", slot(slot_id)),
        style(slot_ink, ground),
    );
    at = lamp(b, r, at, &format!("{glyph} "), style(ink, ground));
    if active {
        let icon = icon(t, chip);
        at = lamp(b, r, at, &format!("{icon} "), style(DATA, ground));
    }
    let id = chip.entry;
    let open = app.transcript().is_some_and(|t| t.opened == Some(id));
    let chosen = matches!(&app.selected, Some(Action::Call(_, at)) if *at == id);
    let name = match (active, open || chosen) {
        (true, true) => bold(LAMP, ground),
        (true, false) => style(DATA, ground),
        (false, _) => style(GRAY_DIM, ground),
    };
    at = lamp(b, r, at, &chip.name, name);
    if let Some(age) = age {
        sparks(b, r, x, age, off);
    }
    at
}

fn phase(
    age: Option<u64>,
    wearing: bool,
    worn: bool,
) -> (&'static str, ratatui::style::Color, f64) {
    let Some(at) = age else {
        return if worn {
            ("○", GRAY_DIM, 0.)
        } else {
            ("◉", LAMP, 0.)
        };
    };
    let stage = (at / 150).min(3) as usize;
    let stage = if wearing { 3 - stage } else { stage };
    let glyph = ["○", "◌", "◎", "◉"][stage];
    let ink = match stage {
        0 => GRAY_DIM,
        3 => LAMP,
        _ => DIM,
    };
    let at = at as f64;
    let glow = if wearing {
        (1. - at / 600.).max(0.)
    } else if at < 450. {
        at / 450.
    } else {
        (1. - (at - 450.) / 550.).max(0.)
    };
    (glyph, ink, glow)
}

fn sparks(b: &mut Buffer, r: Rect, x: u16, age: u64, wearing: bool) {
    let mut mark = |dx: i16, glyph: &str, ink| {
        let col = i32::from(x) + 4 + i32::from(dx);
        if col >= i32::from(r.x) && col < i32::from(r.right()) {
            put(b, col as u16, r.y, glyph, 1, style(ink, RECESS));
        }
    };
    if wearing {
        match age {
            250..550 => mark(0, "˙", GRAY),
            550..850 => mark(1, "˙", GRAY_DIM),
            _ => {}
        }
        return;
    }
    for (start, dx) in [(120, -1), (230, 0), (340, 1)] {
        if let Some(t) = age.checked_sub(start).filter(|t| *t < 380) {
            let stage = (t / 95) as usize;
            mark(
                dx,
                ["⡀", "⠄", "⠂", "⠁"][stage],
                [HI, LAMP, DIM, GRAY_DIM][stage],
            );
        }
    }
}
