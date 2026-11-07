//! The top of the agents list: one line for the servers and one for the residents, each folded
//! to what needs the person (a silent server, a usage window from 70%, a resident asking or
//! failed) and opening into its table.
use super::super::{App, cards, text, text::*, view::Action};
use super::panel::{self, Tone, on};
use super::sections::{Row, addressed, agent_rows, framed};
use crate::protocol::{ResidentDay, RunState, Server, SessionId, State, views::SectionKind};
use ratatui::text::{Line, Span};
use std::collections::{BTreeMap, BTreeSet};

/// The servers line, its table while open, and the residents' panel.
pub(super) fn rows(app: &App, width: usize, now: u64) -> Vec<Row> {
    let mut out = servers(app, width, now);
    out.extend(residents(app, width, now));
    out
}

// The servers line, `▸ servers · all 2 answer · work 7-day 40%`, the fullest window and its
// server's own word, marked as the list's headings are and without its word where that would not
// fit; opened, the machines' table.
fn servers(app: &App, width: usize, now: u64) -> Vec<Row> {
    let usage = usage(app, now);
    let mut line = vec![silent(app, now).map_or_else(
        || span(format!("all {} answer", app.servers.len()), GRAY),
        |s| Span::styled(s, bold(AMBER, GLASS)),
    )];
    if let Some(((server, name), p)) = usage.iter().max_by(|a, b| a.1.total_cmp(b.1)) {
        let whose = (app.servers.iter()).find(|s| s.name == *server);
        let name = format!("{} {name} ", whose.map_or("", realm));
        let name = span(name.trim_start().to_owned(), cards::tone(p / 100., 0.9));
        line.extend([span(" · ", GRAY_DIM), name, reading(Some(*p))]);
    }
    let action = Action::Section("servers".into());
    let open = app.list.toggled.contains("servers");
    let used = spans_width(&line);
    if 2 + "servers".len() + 3 + used <= width {
        line.splice(0..0, [span("servers", GRAY), span(" · ", GRAY_DIM)]);
    }
    let mark = match app.list.open && app.list.selected.as_ref() == Some(&action) {
        true => "❯",
        false => ["▸", "▾"][usize::from(open)],
    };
    line.insert(0, span(format!("{mark} "), GRAY));
    let mut out = vec![Row(Line::from(line), Some(action), None)];
    if !open {
        return out;
    }
    // Right edges of the load, memory, memory used, 5-hour and 7-day columns.
    let wide = width >= 60;
    let (load, mem) = if wide { (31, 24) } else { (18, 12) };
    let [load, mem, used, hour, week] = [load, mem, 17, 7, 1].map(|c| width.saturating_sub(c));
    let head = if wide { "machine / server" } else { "machine" };
    let heads = [1, load, mem, hour, week]
        .into_iter()
        .zip([head, "load", "mem", "5h", "7d"]);
    out.push(placed(heads.map(|(at, s)| (at, span(s, GRAY_DIM)))));
    let machines: BTreeSet<_> = app.servers.iter().map(|s| s.machine.as_str()).collect();
    for machine in machines {
        let mine = app.servers.iter().filter(|s| s.machine == machine);
        let mut cells = vec![(1, Span::styled(machine.to_owned(), bold(WHITE, GLASS)))];
        if let Some(l) = mine.clone().find_map(|s| s.load) {
            let pct = 100. - l.free as f64 * 100. / l.total.max(1) as f64;
            cells.push((load, span(format!("{:.2}", l.cpu), DATA)));
            cells.push((mem, reading(Some(pct))));
            let gib = format!("{}/{}G", (l.total - l.free) >> 30, l.total >> 30);
            cells.extend(wide.then(|| (used, span(gib, GRAY))));
        }
        out.push(placed(cells));
        for s in mine {
            let window = |name| reading(usage.get(&(s.name.as_str(), name)).copied());
            let name = span(format!("  {}", realm(s)), GRAY);
            out.push(match s.silent {
                Some(at) => {
                    let words = format!(" ⊘ {}  no answer {}m", realm(s), age(at, now));
                    Row(Line::from(span(words, AMBER)), None, None)
                }
                None => placed([(0, name), (hour, window("5-hour")), (week, window("7-day"))]),
            });
        }
    }
    out
}

/// The servers not answering, as `⊘ linux·work 3m, mac·home 1m`, each with the minutes since it
/// last did; none while all answer.
pub(super) fn silent(app: &App, now: u64) -> Option<String> {
    let silent: Vec<_> = (app.servers.iter())
        .filter_map(|s| s.silent.map(|at| (s, age(at, now))))
        .map(|(s, age)| format!("{}·{} {age}m", s.machine, realm(s)))
        .collect();
    app.next.every(now, 60_000);
    (!silent.is_empty()).then(|| format!("⊘ {}", silent.join(", ")))
}

/// Each server's highest reading of each usage window among its agents, by server name, leaving
/// out a reading past its reset; the list is drawn again when the soonest kept one resets.
fn usage(app: &App, now: u64) -> BTreeMap<(&str, &str), f64> {
    let mut top = BTreeMap::new();
    for a in app.agents.values() {
        let server = a.session.as_str().split_once('@').map_or("", |s| s.1);
        for w in &a.usage {
            match w.resets_at {
                Some(at) if at <= now => continue,
                Some(at) => app.next.at(at),
                None => {}
            }
            if let Some(u) = w.utilization {
                let at = top.entry((server, w.name.as_str())).or_insert(u);
                *at = at.max(u);
            }
        }
    }
    top
}

/// `work` from the label `box · work`: the server's own part, else its whole label or name.
fn realm(s: &Server) -> &str {
    let label = s.label.rsplit(" · ").next().unwrap_or_default();
    if label.is_empty() { &s.name } else { label }
}

// The residents as a framed panel: its tab and count with the next wake on its top edge, each
// resident's row, its new-thread row and its threads; folded until opened, the top edge alone.
fn residents(app: &App, width: usize, now: u64) -> Vec<Row> {
    let all: Vec<_> = (app.servers.iter())
        .flat_map(|s| s.residents.iter().map(|r| (r.session.on(&s.name), r)))
        .collect();
    if all.is_empty() {
        return Vec::new();
    }
    let (action, tone) = (Action::Section("residents".into()), panel::tone(None));
    let open = app.list.toggled.contains("residents");
    let next = (all.iter().filter_map(|(_, r)| r.upcoming.first())).min_by_key(|(_, at)| at);
    let next = next.map_or(String::new(), |(_, at)| {
        format!(" · NEXT {}", hhmm(app, *at))
    });
    let held = panel::count(all.len(), "RESIDENT");
    let held = format!(" {held}{next} {}", ["▸ ", ""][usize::from(open)]);
    let held = on(held, tone.soft, tone.ground, false);
    let mut top = [panel::shut(width), panel::top(width, true)][usize::from(open)].clone();
    if app.list.open && app.list.selected.as_ref() == Some(&action) {
        top.replace_range(..3, "❯");
    }
    let at = width.saturating_sub(text::width(&held.content) + 2);
    let parts = vec![(2, panel::named(None, "Residents", tone)), (at, vec![held])];
    let top = panel::over(&top, tone.frame, tone.ground, parts);
    let mut out = vec![Row(Line::from(top), Some(action), None)];
    if !open {
        return out;
    }
    for (i, (id, r)) in all.iter().enumerate() {
        let gap = framed(Span::raw(" "), Vec::new(), width, tone, None);
        out.extend((i > 0).then_some(Row(gap, None, None)));
        out.extend(resident(app, (id, r), width, tone, now));
    }
    let foot = panel::foot(width, true, None, [tone.frame, tone.soft, tone.ground]);
    out.push(Row(foot.into(), None, None));
    out
}

// One resident's row (asking, its last run failed, or working or idle with its next wake) and,
// unless its block is shut, the new-thread row and its threads.
fn resident(
    app: &App,
    (id, r): (&SessionId, &ResidentDay),
    width: usize,
    tone: Tone,
    now: u64,
) -> Vec<Row> {
    let (agent, g) = (app.agents.get(id), tone.ground);
    let (mark, words, ink) = if agent.is_some_and(|a| !a.asks.is_empty()) {
        ("◆", "asking".into(), AMBER)
    } else if let Some((wake, _, RunState::Failed)) = r.runs.first() {
        ("▲", format!("{wake} failed"), RED)
    } else {
        let next = r
            .upcoming
            .first()
            .map(|(wake, at)| format!("{wake} {}", hhmm(app, *at)));
        let working = agent.is_some_and(|a| a.state == State::Working);
        let (mark, ink) = [("○", GRAY), ("●", GREEN)][usize::from(working)];
        (mark, next.unwrap_or_default(), ink)
    };
    let selected = app.list.selected == Some(Action::Agent(id.clone()));
    // The resident's block folds as a parent's children do, by its home session's id.
    let (fold, kids) = (id.to_string(), app.list.layout().into_iter());
    let shut = app.list.toggled.contains(&fold);
    let kids = (kids.flat_map(|l| &l.sections)).filter(|s| s.kind == SectionKind::Residents);
    let kids = kids
        .flat_map(|s| &s.sessions)
        .skip_while(|r| &r.session != id)
        .skip(1);
    let kids: Vec<_> = kids.take_while(|r| r.depth > 0).collect();
    let room = width.saturating_sub(10 + text::width(&words));
    let name = clip(r.session.as_str(), room);
    let gap = " ".repeat(room.saturating_sub(text::width(&name)) + 1);
    let arrow = ["▾ ", "▸ "][usize::from(shut)];
    let content = vec![
        on(arrow, [GRAY, CYAN][usize::from(selected)], g, true),
        on(format!("{mark} "), ink, g, false),
        on(name, WHITE, g, true),
        on(format!("{gap}{words}"), GRAY, g, false),
    ];
    let mut line = framed(Span::raw(" "), content, width, tone, None);
    let hides = kids.iter().any(|r| app.focus.as_ref() == Some(&r.session));
    addressed(&mut line, app.focus.as_ref() == Some(id) || shut && hides);
    let (press, fold) = (Action::Agent(id.clone()), Action::Section(fold));
    let mut body = vec![Row(line, Some(press), Some(fold))];
    if shut {
        return body;
    }
    let new = Action::Thread(id.clone());
    let picked = app.list.selected.as_ref() == Some(&new);
    let lead = on([" ", "❯"][usize::from(picked)], CYAN, g, true);
    let content = vec![on("  + New thread", CYAN, g, false)];
    let line = framed(lead, content, width, tone, None);
    body.push(Row(line, Some(new), None));
    body.extend(agent_rows(app, kids, width, tone, now));
    body
}

/// `14:00` in the terminal's zone; empty without one.
pub(super) fn hhmm(app: &App, at: u64) -> String {
    zoned(at, app.zone.as_ref()).map_or_else(String::new, |t| t.strftime("%H:%M").to_string())
}

/// Cells left to right, each ending at its column where it fits, and at least one space after
/// the one before it (the first, after the edge).
fn placed(cells: impl IntoIterator<Item = (usize, Span<'static>)>) -> Row {
    let mut x = 0;
    let mut out = Vec::new();
    for (at, s) in cells {
        let n = text::width(&s.content);
        let start = at.saturating_sub(n).max(x + 1);
        out.push(span(" ".repeat(start - x), GRAY));
        x = start + n;
        out.push(s);
    }
    Row(Line::from(out), None, None)
}

/// A percentage, amber from 70 and red from 90; `–` when unmeasured.
fn reading(p: Option<f64>) -> Span<'static> {
    p.map_or(span("–", GRAY_DIM), |p| {
        span(format!("{p:.0}%"), cards::tone(p / 100., 0.9))
    })
}

/// Whole minutes since `at`.
fn age(at: u64, now: u64) -> u64 {
    now.saturating_sub(at) / 60_000
}
