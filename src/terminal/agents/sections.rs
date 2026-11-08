//! The list's rows under its header: each top-level section a framed panel in its group's
//! colour with its subgroups' headings, each agent on two rows with its helpers branching under
//! it, from the server's layout with local folding only.
use super::super::text::*;
use super::super::{App, cards, view::Action};
use super::panel::{self, Spans, Tone, on, standing};
use super::servers;
use crate::protocol::AgentView;
use crate::protocol::views::{Section, SectionKind, ViewRow};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

/// The rows under the list's header: the servers line, the residents, a new agent while the list
/// has the keys, then every section of the layout but the residents'.
pub fn rows(app: &App, width: u16, now: u64) -> Vec<Row> {
    let w = width as usize;
    let mut out = servers::rows(app, w, now);
    if app.list.open {
        let mark = ["+", "❯"][usize::from(app.list.selected == Some(Action::New))];
        let label = app.servers.iter().find(|s| s.name == app.server);
        let label = label.map_or("", |s| s.label.as_str());
        let aimed = app.cwd.file_name().filter(|_| app.focus.is_none());
        let aimed = aimed.map(|f| format!(" in {} on {label}", f.to_string_lossy()));
        let new = format!(" {mark} New agent{}", aimed.unwrap_or_default());
        out.push(row(span(clip(&new, w), CYAN).into(), Some(Action::New)));
    }
    let Some(layout) = app.list.layout() else {
        let text = app.list.status.as_ref().and_then(|s| s.error.as_deref());
        let text = clip(text.unwrap_or("Views are loading…"), w.saturating_sub(1));
        out.push(row(span(format!(" {text}"), GRAY).into(), None));
        return out;
    };
    let numbered = super::overview::groups(app);
    let number = |id: &str| numbered.iter().position(|t| t[0].id == id).map(|i| i + 1);
    let (sections, mut start) = (&layout.sections, 0);
    while start < sections.len() {
        let tree = subtree(&sections[start..]);
        start += tree.len();
        if tree[0].kind != SectionKind::Residents {
            out.extend(group(app, tree, number(&tree[0].id), w, now));
        }
    }
    out
}
/// One list row, its press, and the press of the fold mark in its second and third columns.
pub struct Row(pub Line<'static>, pub Option<Action>, pub Option<Action>);

fn row(line: Line<'static>, action: Option<Action>) -> Row {
    Row(line, action, None)
}
// Whether `s` is folded: Earlier opened, any other closed.
fn folded(app: &App, s: &Section) -> bool {
    (s.kind == SectionKind::Earlier) != app.list.toggled.contains(&s.id)
}

// A top-level section as a framed panel: its top edge with its tabs, how many need the person
// and its agents; its subgroups and agents; its foot. Folded, the top edge alone, with its
// census and its corners open.
fn group(app: &App, tree: &[Section], number: Option<usize>, w: usize, now: u64) -> Vec<Row> {
    let (root, tone) = (&tree[0], panel::tone(tree[0].colour));
    let action = Action::Section(root.id.clone());
    let shut = folded(app, root);
    let roots = panel::rows(tree).filter(|r| r.depth == 0);
    let census = panel::census(roots.filter_map(|r| app.agent_row(&r.session)));
    let mut tags = panel::named(number, &root.name, tone);
    let needs = format!(" ◆ {} ", census[0] + census[1]);
    tags.extend((census[0] + census[1] > 0).then(|| on(needs, AMBER, tone.ground, true)));
    let agents = if root.kind == SectionKind::Earlier {
        if shut {
            String::new()
        } else {
            format!("{} LOADED", root.sessions.len())
        }
    } else {
        panel::count(totals(tree).0, "AGENT")
    };
    if shut {
        let census = [0, census[1], census[2], census[3]];
        tags.extend(panel::counts(census, tone.ground));
    }
    let here = shut && contains(app, tree);
    let right = format!(" {agents} {}", [" ", "▸ "][usize::from(shut)]);
    let right = on(
        right,
        [tone.soft, CYAN][usize::from(here)],
        tone.ground,
        false,
    );
    let at = w.saturating_sub(width(&right.content) + 1 + usize::from(!shut));
    let edge = [panel::top(w, true), panel::shut(w)][usize::from(shut)].clone();
    let ink = [tone.frame, mix(tone.ground, tone.hue, 0.35)][usize::from(shut)];
    let mut top = panel::over(&edge, ink, tone.ground, vec![(2, tags), (at, vec![right])]);
    if app.list.selected.as_ref() == Some(&action) {
        top[0].content = top[0].content.chars().skip(1).collect::<String>().into();
        top.insert(0, on("❯", CYAN, tone.ground, true));
    }
    let mut out = vec![row(top.into(), Some(action))];
    if shut {
        return out;
    }
    let (mut hidden, mut subgroups) = (None::<u16>, usize::from(!root.sessions.is_empty()));
    for (i, s) in tree.iter().enumerate() {
        if hidden.is_some_and(|depth| s.depth > depth) {
            continue;
        }
        let shut = i > 0 && folded(app, s);
        hidden = shut.then_some(s.depth);
        if i > 0 {
            if subgroups > 0 {
                out.push(row(framed(Span::raw(" "), Vec::new(), w, tone, None), None));
            }
            subgroups += 1;
            out.push(heading(app, subtree(&tree[i..]), shut, w, tone));
        }
        if !shut {
            out.extend(agent_rows(app, s.sessions.iter(), w, tone, now));
        }
    }
    if root.kind == SectionKind::Earlier {
        out.extend(super::earlier::controls(app, w, tone));
    }
    let foot = panel::foot(w, true, number, [tone.frame, tone.soft, tone.ground]);
    out.push(row(foot.into(), None));
    out
}

// A subgroup's heading: its fold mark (cyan when selected), its name, a rule in the group's
// colour and its census by state.
fn heading(app: &App, tree: &[Section], shut: bool, w: usize, tone: Tone) -> Row {
    let (s, g) = (&tree[0], tone.ground);
    let action = Action::Section(s.id.clone());
    let lit = app.list.selected.as_ref() == Some(&action);
    let (indent, mark) = (
        "  ".repeat(s.depth as usize - 1),
        ["▾", "▸"][usize::from(shut)],
    );
    let mark = format!("{indent}{mark} ");
    let mark = on(mark, [tone.soft, CYAN][usize::from(lit)], g, true);
    let name = format!("{} ", s.name.to_uppercase());
    let name = on(name, mix(tone.hue, WHITE, 0.45), g, true);
    let roots = panel::rows(tree).filter(|r| r.depth == 0);
    let census = panel::census(roots.filter_map(|r| app.agent_row(&r.session)));
    let census = panel::counts(census, g);
    let used = width(&mark.content) + width(&name.content) + spans_width(&census);
    let rule = format!("{} ", "─".repeat(w.saturating_sub(5 + used)));
    let rule = on(rule, mix(g, tone.hue, 0.3), g, false);
    let content = [vec![mark, name, rule], census].concat();
    let mut line = framed(Span::raw(" "), content, w, tone, None);
    addressed(&mut line, shut && contains(app, tree));
    row(line, Some(action))
}

/// A row inside a frame of `tone`, `w` columns: its sides, `lead` in the margin column, then
/// `content` padded on the frame's ground to the right side, which `arrow` replaces.
pub fn framed(
    lead: Span<'static>,
    content: Spans,
    w: usize,
    tone: Tone,
    arrow: Option<Span<'static>>,
) -> Line<'static> {
    let side = on("│", tone.frame, tone.ground, false);
    let lead = Span::styled(lead.content, lead.style.bg(tone.ground));
    let content = panel::ends(content, Vec::new(), w.saturating_sub(3), tone.ground);
    let mut out = [vec![side.clone(), lead], content].concat();
    out.push(arrow.unwrap_or(side));
    Line::from(out)
}

pub(super) fn totals(sections: &[Section]) -> (usize, usize) {
    let agents = panel::rows(sections).filter(|r| r.depth == 0).count();
    (agents, panel::rows(sections).count() - agents)
}
pub(super) fn subtree(sections: &[Section]) -> &[Section] {
    let below = sections[1..]
        .iter()
        .take_while(|s| s.depth > sections[0].depth);
    &sections[..1 + below.count()]
}
fn contains(app: &App, sections: &[Section]) -> bool {
    panel::rows(sections).any(|r| app.focus.as_ref() == Some(&r.session))
}
/// Marks the row that is, or hides, the session the composer addresses; the list names it at its
/// foot while it is scrolled away.
pub fn addressed(line: &mut Line, here: bool) {
    if here {
        line.style.bg = Some(YOU_BG);
    }
}
/// Whether the row is, or hides, the session the composer addresses.
pub fn here(line: &Line) -> bool {
    line.style.bg == Some(YOU_BG)
}
/// The local fold of `a`'s folded finished siblings, closed until opened.
pub(super) fn finished(a: &AgentView) -> String {
    format!("finished/{}", a.parent.as_ref().map_or("", |p| p.as_str()))
}
// A row under a heading as the list shows it: the fold its Space toggles (a parent's children,
// or the folded finished children a line stands for) and whether it is shut; with a count, that
// line instead of an agent's rows; and whether the row is, or hides, the session the composer
// addresses.
type Shown<'a> = (&'a ViewRow, Option<(String, bool)>, Option<usize>, bool);
fn shown<'a>(app: &'a App, rows: impl Iterator<Item = &'a ViewRow>) -> Vec<Shown<'a>> {
    let rows: Vec<_> = rows
        .filter(|r| app.agent_row(&r.session).is_some())
        .collect();
    // Shut below this depth, and at it for folded rows while their finished line is shut.
    let (mut out, mut hidden) = (Vec::<Shown>::new(), None::<(u16, bool)>);
    for (i, r) in rows.iter().copied().enumerate() {
        let here = app.focus.as_ref() == Some(&r.session);
        let shut = |(depth, folded)| r.depth > depth || folded && r.depth == depth && r.folded;
        if hidden.is_some_and(shut) {
            out.last_mut().into_iter().for_each(|hider| hider.3 |= here);
            continue;
        }
        // A parent's folded finished children follow the others; the first heads their line.
        let previous = rows[..i].iter().rev().find(|p| p.depth <= r.depth);
        if r.folded && previous.is_none_or(|p| p.depth < r.depth || !p.folded) {
            let id = finished(app.agent_row(&r.session).unwrap());
            let siblings = rows[i..].iter().take_while(|n| n.depth >= r.depth);
            let count = siblings.filter(|n| n.depth == r.depth).count();
            let shut = !app.list.toggled.contains(&id);
            out.push((r, Some((id, shut)), Some(count), here && shut));
            if shut {
                hidden = Some((r.depth, true));
                continue;
            }
        }
        let parent = rows.get(i + 1).is_some_and(|next| next.depth > r.depth);
        let id = r.session.to_string();
        let fold = parent.then(|| (id.clone(), app.list.toggled.contains(&id)));
        hidden = fold
            .as_ref()
            .and_then(|(_, shut)| shut.then_some((r.depth, false)));
        out.push((r, fold, None, here));
    }
    out
}

/// The rows of `rows` inside a frame of `tone`: each root's name and status rows, each helper's
/// row branching under its root, and each folded finished line, all on the root's ground with
/// its edge down their left.
pub fn agent_rows<'a, R>(app: &'a App, rows: R, w: usize, tone: Tone, now: u64) -> Vec<Row>
where
    R: IntoIterator<Item = &'a ViewRow>,
{
    let shown = shown(app, rows.into_iter());
    let depths = shown
        .iter()
        .filter(|s| s.0.depth > 0)
        .map(|s| s.0.depth as usize);
    let mut branches = cards::connectors(&depths.collect::<Vec<_>>(), false, w).into_iter();
    let (mut out, mut ground, mut edge) = (Vec::new(), tone.ground, tone.frame);
    let room = w.saturating_sub(4);
    for (at, (r, fold, count, here)) in shown.iter().enumerate() {
        let a = app.agent_row(&r.session).unwrap();
        let toggle = count
            .and(fold.as_ref())
            .map(|f| Action::Section(f.0.clone()));
        let action = toggle.or(Some(Action::Agent(a.session.clone())));
        let lit = app.list.open && app.list.selected == action;
        let lead = [Span::raw(" "), on("❯", CYAN, tone.ground, true)][usize::from(lit)].clone();
        if r.depth == 0 {
            let s = standing(a);
            (ground, edge) = panel::ground(app, s, tone, now);
            let band = mix(ground, WHITE, 0.05);
            let below = shown[at + 1..]
                .iter()
                .take_while(|n| n.0.depth > 0)
                .map(|n| n.0);
            let shut = fold.as_ref().is_some_and(|f| f.1);
            let marks = if shut {
                panel::marks(app, below, now)
            } else {
                Vec::new()
            };
            let quiet = mix(mix(DATA, PANEL_INK, 0.25), tone.hue, 0.25);
            let goal = vec![on(format!("  {}", a.headline), quiet, band, false).italic()];
            let head = panel::head(app, a, (marks, goal), true, room - 1, (band, *here), now);
            let arrow = here.then(|| on("▶", CYAN, tone.ground, false));
            let head = [vec![on("▌", edge, band, false)], head].concat();
            let mut name = framed(lead.clone(), head, w, tone, arrow);
            addressed(&mut name, *here);
            out.push(row(name, action.clone()));
            let (lead, lead_ink, words, ink) = s.said(a);
            let bars = panel::bars(app, a, 8, tone.hue, now).0.into_iter();
            let bars = bars.map(|b| Span::styled(b.content, b.style.bg(ground)));
            let lead = on(format!(" {lead} "), lead_ink, ground, false);
            let said = vec![lead, Span::styled(words, ink.bg(ground))];
            let said = panel::ends(said, bars.collect(), room - 1, ground);
            let said = [vec![on("▌", edge, ground, false)], said].concat();
            out.push(row(framed(Span::raw(" "), said, w, tone, None), action));
            continue;
        }
        let branch = branches.next().unwrap_or_default();
        let branch = format!("  {}", &branch[1..]);
        let branch = on(branch, mix(ground, tone.hue, 0.5), ground, false);
        let mut content = vec![on("▌", edge, ground, false), branch];
        match count {
            Some(n) => content.push(on(format!("✓ {n} done"), PANEL_GRAY, ground, false)),
            None => {
                let (mark, word) = panel::helper(app, a, now);
                let ink = [PANEL_GRAY, AMBER][usize::from(word.starts_with("waiting"))];
                let name = format!(" {}  ", a.name);
                let name = on(name, [DATA, ink][usize::from(ink == AMBER)], ground, false);
                let mark = Span::styled(mark.content, mark.style.bg(ground));
                content.extend([mark, name, on(word, ink, ground, false)]);
            }
        }
        let content = panel::ends(content, Vec::new(), room, ground);
        let mut line = framed(lead, content, w, tone, None);
        addressed(&mut line, *here);
        out.push(row(line, action));
    }
    out
}
