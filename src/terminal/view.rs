//! One frame: the bar, the gauge, the transcript, the band above the composer, the composer
//! and the status pane, or the split while a child is addressed. Each click target is a hit.
use super::keys::Pane;
use super::{
    App, bindings, children, composer, gauge, motion, panel, picker, questions, split, status,
    strip, text::*, transcript,
};
use crate::protocol::{Body, EntryId, Input, MailId, Quit, SessionId, State};
use composer::{completion, emoji, shortcodes};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use std::sync::Arc;

/// Below this many columns the frame is the phone's: no gauge tape, the flat gauge on the row
/// under the bar, the band's rule always drawn, the composer's two-row form, and the pane
/// floating at the full width. Columns this narrow draw the reply rail and keys short too.
pub(super) const PHONE: u16 = 70;

/// A drawn click target and what a click there does; where targets overlap, the last drawn wins.
pub(super) type Hit = (Rect, Action);
#[derive(Clone, PartialEq)]
pub(super) enum Action {
    /// The Voice chip: starts a voice note, as Ctrl-R does.
    Record,
    /// A voice note's ▶, or the newest completed reply's: plays or stops it.
    Play(SessionId, EntryId),
    Child(SessionId),
    Archive,
    Section(String),
    Agent(SessionId),
    Earlier,
    /// The agents list's "+ New agent" row: opens the new-agent chooser.
    New,
    /// The agents list's right edge: a press starts widening or narrowing the list.
    Edge,
    /// The agents list's ⤢: opens the overview, as F3 does.
    Overview,
    /// The agents list's ‹: closes it, as F2 does.
    Close,
    /// A resident's "+ New thread" row, by its home session: aims the composer at a fresh thread.
    Thread(SessionId),
    /// A project row of the new-agent chooser, or one of its servers, by server and folder as
    /// drawn: aims the new agent's composer there.
    Choose(String, std::path::PathBuf),
    /// The inbox folded to its newest four, or opened at a page of fifteen from this offset.
    Inbox(Option<usize>),
    /// The tasks pane folded, or opened at a page of fifteen open and closed tasks.
    Tasks(Option<usize>),
    /// Opens a task card in the tasks panel.
    Task(u64),
    Delivery(SessionId, EntryId, Option<Arc<str>>),
    Option(MailId, usize),
    Call(SessionId, EntryId),
    Back,
    Status,
    Model,
    /// Words a press copies: an ask's, or a delivered file's path.
    Copy(String),
    CopyText(SessionId, EntryId),
    RemoveImage(usize),
    /// The ☺ chip: opens or closes the emoji grid.
    Emoji,
    /// The children strip's line: opens or folds its list.
    Strip,
    /// A row of the completion list: completes it into the draft, as Tab does on that row.
    Complete(usize),
    /// A cell, tab or tone of the open emoji grid.
    Grid(composer::emoji::Hit),
    /// A match of the shortcode strip.
    Code(usize),
    /// A queued row, or the "+N more queued" row for the newest: takes that input back.
    TakeBack(EntryId),
    /// The chip on copied words: quotes them into the editor.
    Quote,
    /// A cell of the session map, or the pin: jumps the transcript to that entry.
    Jump(SessionId, EntryId),
    /// A reply's cap row, or the pin's label: pins or unpins that reply.
    Pin(SessionId, EntryId),
    /// A server's row in the `/mcp` list: chooses and opens it.
    Mcp(String),
    /// An ask's web address in the band: an OSC 8 link, and a click copies it.
    Link(std::sync::Arc<str>),
    /// The newer entries' line: brings the transcript to its latest, as Ctrl-End does.
    Latest(SessionId),
    /// A chip in the image tray: the tray's keys go to it.
    Chip(usize),
    /// A folded run's `looked` row: draws the run's calls again.
    Unfold(SessionId, EntryId),
    /// The band's one row: opens that ask to its options, as a first digit does.
    Ask(MailId),
    /// An option the band cut short: a click says it cannot be picked.
    Cut,
    /// The ⎘ Image chip: says how an image attaches.
    Image,
    /// The Send chip: sends as Enter does, or says there is nothing yet.
    Send,
    /// A worn handed skill's line: hands that skill to its transcript's session again, the
    /// person's own act.
    HandBack(SessionId, String),
}

/// Draws the frame into `b`, records its hits and links, and returns the editor's cursor.
pub(super) fn draw(app: &mut App, b: &mut Buffer, area: Rect, now: u64) -> Option<(u16, u16)> {
    fill(b, area, style(DATA, GLASS));
    (app.status_area, app.columns) = (None, area.width);
    app.links.clear();
    app.visible_options.clear();
    app.targets.clear();
    let question = app.question().cloned();
    if question.as_ref().map(|q| &q.id) != app.shown_ask.as_ref().map(|a| &a.id) {
        app.ask_at = now;
    }
    app.shown_ask = question;
    let mut hits = Vec::new();
    // Under the overview at rest nothing else is drawn.
    let mut cursor = match () {
        _ if super::agents::covers(app, now) => None,
        _ if app.list.open || app.list.area.is_some() => {
            super::agents::frame(app, b, area, now, &mut hits)
        }
        _ if app.split() && app.keys == Pane::Status => {
            let width = if area.width < 80 {
                area.width
            } else {
                (area.width / 4).clamp(38, 46)
            };
            let body = Rect {
                width: area.width - width,
                ..area
            };
            if body.width > 0 {
                split::draw(app, b, body, now, &mut hits);
            }
            let pane = Rect::new(body.right(), area.y, width, area.height);
            status_side(app, b, pane, now, &mut hits);
            None
        }
        _ if app.split() => split::draw(app, b, area, now, &mut hits),
        _ => normal(app, b, area, now, &mut hits),
    };
    if split::closing(app, b, now, &mut hits).is_some() {
        cursor = None;
    }
    if super::agents::overlay(app, b, now, &mut hits) {
        cursor = None;
    }
    super::audio::controls(app, b, &mut hits);
    for (r, action) in &hits {
        if let Action::Link(url) | Action::Delivery(_, _, Some(url)) = action {
            app.links.push((*r, url.clone()));
        }
    }
    if app.picker.is_some() || app.panel.is_some() || app.keys != Pane::Conversation {
        cursor = None;
    }
    if app.goto.is_some() {
        cursor = super::goto::draw(app, b, area, now);
    }
    match app.picker.is_some() || app.goto.is_some() {
        true => app.links.clear(),
        false => bare(b, &mut app.links),
    }
    super::brand::paint(&app.brand, b);
    ring(app, b, &hits);
    app.hits = hits;
    transcript::select::paint(app, b, now);
    cursor
}

// Tab's ring from this frame's targets, less those their own keys reach (the emoji grid, the
// shortcode and completion lists, /mcp's list and the session map), and the browse cursor's mark
// on the one selected; a selection the ring no longer holds is dropped.
fn ring(app: &mut App, b: &mut Buffer, hits: &[Hit]) {
    // The normal frame gave Tab's ring its order; the split's is its draw order.
    let ring = match app.targets.is_empty() {
        true => hits.iter().map(|(_, a)| a.clone()).collect(),
        false => std::mem::take(&mut app.targets),
    };
    for action in ring {
        let own = matches!(
            action,
            Action::Grid(_)
                | Action::Code(_)
                | Action::Complete(_)
                | Action::Mcp(_)
                | Action::Jump(..)
        );
        if !own && !app.targets.contains(&action) {
            app.targets.push(action);
        }
    }
    app.selected = app.selected.take().filter(|a| app.targets.contains(a));
    // The browse cursor's target shows the cards' mark: a row's in its second column, a chip's
    // in its first; a child card draws its own.
    if let Some(selected) = &app.selected
        && let Some((r, _)) = hits.iter().rev().find(|(_, a)| a == selected)
    {
        let row = matches!(selected, Action::Call(..) | Action::CopyText(..));
        let cell = b.cell_mut((r.x + u16::from(row), r.y));
        if let (false, Some(cell)) = (matches!(selected, Action::Child(_)), cell) {
            cell.set_symbol("›")
                .set_fg(CYAN)
                .modifier
                .insert(Modifier::BOLD);
        }
    }
}

// Columns: the gauge's tape, the body, and the status pane docked from 140 columns (else it
// floats over the transcript's right side). Rows: the bar (on a phone with the gauge under it),
// then the transcript, the band and the composer, which take their heights first.
pub(super) fn normal(
    app: &mut App,
    b: &mut Buffer,
    area: Rect,
    now: u64,
    hits: &mut Vec<Hit>,
) -> Option<(u16, u16)> {
    let phone = area.width < PHONE;
    let reader = matches!(app.panel, Some(super::panel::Panel::Delivery(_)));
    if area.width < 80 && app.keys == Pane::Status {
        status_side(app, b, area, now, hits);
        return None;
    }
    let docked = match docked(app) || reader && area.width >= 80 {
        true => (area.width / 4).clamp(38, 46),
        false => 0,
    };
    // The agents view and a phone read the context in the bar; otherwise the tape stands left.
    let tape = match phone || app.list.open || app.list.area.is_some() {
        true => 0,
        false => gauge::columns(app).min(area.width),
    };
    let (x, width) = (area.x + tape, area.width.saturating_sub(tape + docked));
    let top = Rect::new(area.x, area.y, area.width, area.height.min(4));
    let (y, mid) = (area.y + top.height, area.height - top.height);
    let band = band(app, width, area.height, now).min(mid);
    let card = match phone {
        true => app.composer.phone_rows(app, width, area.height),
        false => app.composer.rows(app, width, area.height),
    };
    let card = card.min(mid - band);
    let rows = mid - band - card;
    let transcript = Rect::new(x, y, width, rows.max(1.min(mid)));
    let (stack, editor) = (
        Rect::new(x, y + rows, width, band),
        Rect::new(x, y + rows + band, width, card),
    );
    let agent = app.focus.as_ref().and_then(|id| app.agent_row(id));
    super::topbar::draw(app, b, top, agent, tape == 0, now);
    if tape > 0 {
        let column = Rect::new(area.x, y, tape, mid);
        gauge::draw(app, b, column, app.focus.as_ref(), now);
    }
    let start = hits.len();
    if let Some(id) = app.focus.clone() {
        self::transcript(app, b, transcript, &id, now, hits);
    }
    let band = hits.len();
    self::stack(app, b, stack, now, hits);
    let card = hits.len();
    let cursor = match phone {
        true => app.composer.phone(app, b, editor, hits),
        false => app.composer.draw(app, b, editor, now, hits),
    };
    let over = Rect::new(x, y, width, editor.y.saturating_sub(y));
    picker::draw(app, b, over);
    super::start::draw(app, b, over, hits);
    let side = hits.len();
    let pane = match docked {
        // A floating pane is at most 46 wide, the full width on a phone.
        0 if (app.keys == Pane::Status || app.pane_open.unwrap_or(area.width >= 140))
            && !reader =>
        {
            let w = [area.width.min(46), area.width][usize::from(phone)];
            Some(Rect::new(area.right() - w, y, w, transcript.height))
        }
        0 => None,
        _ => Some(Rect::new(x + width, y, docked, mid)),
    };
    if let Some(pane) = pane {
        status::draw(app, b, pane, now, hits);
        if docked == 0 && pane.width >= 12 {
            float(b, pane, hits);
        }
    }
    // Tab's ring: the band, the transcript newest first, the pane, then the composer's chips.
    let at = |r: std::ops::Range<usize>| hits[r].iter().map(|(_, a)| a.clone());
    let newest = at(start..band).rev();
    let ring = at(band..card).chain(newest).chain(at(side..hits.len()));
    app.targets = ring.chain(at(card..side)).collect();
    cursor
}

// Bare web addresses in drawn cells are links too; an existing link keeps its own address.
fn bare(b: &Buffer, links: &mut Vec<(Rect, std::sync::Arc<str>)>) {
    let a = b.area;
    for y in a.y..a.bottom() {
        let mut x = a.x;
        while x < a.right() {
            let at = |x: u16| b[(x, y)].symbol();
            let starts = |p: &str| {
                (x..a.right())
                    .take(p.len())
                    .map(at)
                    .eq(p.split_inclusive(|_| true))
            };
            if at(x) != "h" || !(starts("https://") || starts("http://")) {
                x += 1;
                continue;
            }
            let stop = |c: &str| c.trim().is_empty() || matches!(c, "\"" | "'" | "<" | ">" | "│");
            let end = (x..a.right()).find(|x| stop(at(*x))).unwrap_or(a.right());
            let mut url: String = (x..end).map(at).collect();
            while url.ends_with(['.', ',', ';', ':', '!', '?', ')']) {
                url.pop();
            }
            if !links
                .iter()
                .any(|(r, _)| r.y == y && r.x <= x && x < r.right())
            {
                links.push((Rect::new(x, y, width(&url) as u16, 1), url.into()));
            }
            x = end;
        }
    }
}

// A floating pane's shadow, `░▒▓` over the three columns left of it (only the symbol and its
// colour, so the ground stays the transcript's), and its ` ✕ close ` button on the tab row.
fn float(b: &mut Buffer, pane: Rect, hits: &mut Vec<Hit>) {
    let shades = [("░", GRAY_DIM), ("▒", GRAY), ("▓", GRAY)];
    for (x, (glyph, fg)) in (pane.x.saturating_sub(3)..pane.x).zip(shades) {
        for y in pane.y..pane.bottom() {
            if let Some(cell) = b.cell_mut((x, y)) {
                cell.set_symbol(glyph).set_fg(fg).modifier = Modifier::empty();
            }
        }
    }
    let close = [" ✕ ", " ✕ close "][usize::from(pane.width >= 18)];
    let w = width(close) as u16;
    let at = Rect::new(pane.right() - w, pane.y, w, 1);
    put(b, at.x, at.y, close, w, style(DATA, BEZEL));
    hits.push((at, Action::Status));
}

/// The band's height for a body `width` by `height` at `now`: the rule or notice, the lamps,
/// the question, the open list or grid, the queued inputs (three rows at most, and the one
/// leaving), the image tray and the children strip. The normal frame and the split both ask
/// it, so neither misses an item.
pub(super) fn band(app: &App, width: u16, height: u16, now: u64) -> u16 {
    let asked = (app.shown_ask.as_ref()).map_or(0, |q| questions::rows(app, q, width, height));
    let queued = queued(app, now).len() as u16;
    let lamps = super::tools::lamps::rows(app.transcript());
    let images = composer::image_rows(&app.composer);
    let pin = pin(app, width).map_or(0, |(_, _, reply)| reply.lines.len() as u16);
    let (recap, more) = (super::recap::lines(app, width).len() as u16, overlay(app));
    let unanswered = u16::from(unanswered(app).is_some());
    let audio = warned(app, width.into()).len() as u16;
    [lamps, pin, asked, more, queued, unanswered, images, recap]
        .into_iter()
        .fold(1 + strip::rows(app, now) + audio, u16::saturating_add)
}

/// Blends every cell of `r` toward `fg` and `bg` by `k`: 0 leaves the cells as drawn, 1
/// paints them over. A row lights and fades with it, and a new row fades in from the glass.
pub(super) fn tint(b: &mut Buffer, r: Rect, fg: Color, bg: Color, k: f64) {
    for y in r.y..r.bottom().min(b.area.bottom()) {
        for x in r.x..r.right().min(b.area.right()) {
            let cell = &mut b[(x, y)];
            (cell.fg, cell.bg) = (mix(cell.fg, fg, k), mix(cell.bg, bg, k));
        }
    }
}

/// Whether the status pane is docked beside the body: from 140 columns, unless closed.
pub(super) fn docked(app: &App) -> bool {
    app.columns >= 140 && app.pane_open.unwrap_or(true)
}

/// Draws `id`'s transcript, or the open panel over the addressed one, and its rows' hits;
/// a launch row opens its child.
pub(super) fn transcript(
    app: &mut App,
    b: &mut Buffer,
    r: Rect,
    id: &SessionId,
    now: u64,
    hits: &mut Vec<Hit>,
) {
    if app.focus.as_ref() == Some(id) && app.panel.is_some() {
        return panel::draw(app, b, r, now, hits);
    }
    let agent = app.agent_row(id);
    let model = agent
        .map_or(app.model.as_str(), |a| a.model.as_str())
        .to_owned();
    let identity = agent.map(|a| (a.window, a.name.clone()));
    let working = agent.is_some_and(|a| a.state == State::Working);
    let takes = super::inputs::takes(agent);
    let skills = agent
        .filter(|a| {
            app.transcripts
                .get(id)
                .is_some_and(|t| t.skills != a.skills)
        })
        .map(|a| a.skills.clone());
    let asks = agent.map_or_else(Vec::new, |a| a.asks.iter().map(|q| q.id.1).collect());
    // A rejected usage window not yet past its reset: the latest reset, if any is given.
    let usage = agent.map_or(&[][..], |a| &a.usage[..]);
    let rejected = (usage.iter().filter(|w| w.rejected))
        .map(|w| w.resets_at)
        .filter(|at| at.is_none_or(|at| at > now))
        .max();
    let names = app
        .agent_children(id)
        .take(1024)
        .map(|a| {
            (
                a.session.clone(),
                (a.name.clone(), super::text::model(&a.model)),
            )
        })
        .collect();
    children::prepare(app, id, r.width.saturating_sub(2), now);
    transcript::map::show(app, id);
    let (split, motion) = (app.split_view(), super::motion::on(app));
    let stop = (!split).then(|| super::bindings::hint(app, "composer.back"));
    let Some(t) = app.transcripts.get_mut(id) else {
        return;
    };
    t.stops(stop.as_deref().unwrap_or_default());
    t.takes(takes);
    if let Some(skills) = skills {
        t.skills(skills);
    }
    let band = app.shown_ask.as_ref().filter(|q| &q.id.0 == id);
    t.asks(asks, band.map(|q| q.id.1));
    t.limit(rejected, now);
    t.names(
        names,
        id.as_str().split_once('@').map_or("", |(_, server)| server),
    );
    if t.zone.is_none() {
        t.zone = app.zone.clone();
    }
    if let Some((window, name)) = identity {
        t.window = window;
        t.name = name;
    }
    t.motion(motion, app.presence);
    t.frame(app.pictures.cell, app.pictures.frame);
    let mut acts = t.draw(b, r, &model, working, now, &mut app.links);
    if app.columns >= 120 {
        t.map(b, r, &mut acts);
    }
    app.next.at(t.next());
    hits.extend(
        acts.into_iter()
            .map(|(action, area)| (area, press(app, id, action))),
    );
}

// What a press on a row of `id`'s transcript does; a launch row opens its child.
fn press(app: &App, id: &SessionId, action: transcript::Action) -> Action {
    let id = id.clone();
    match action {
        transcript::Action::Play(entry) => Action::Play(id, entry),
        transcript::Action::Copy(entry) => Action::CopyText(id, entry),
        transcript::Action::Jump(entry) => Action::Jump(id, entry),
        transcript::Action::Pin(entry) => Action::Pin(id, entry),
        transcript::Action::Latest => Action::Latest(id),
        transcript::Action::HandBack(entry) => {
            // The skill an input handed over, or one a worn-off skill call loaded.
            let body = app
                .transcripts
                .get(&id)
                .and_then(|t| Some(&t.entries.get(&entry)?.body));
            let name = body.and_then(|body| match body {
                Body::Input(input) => super::tools::skill::input(input).map(|(name, _, _)| name),
                Body::Call { input, .. } => input["name"].as_str(),
                _ => None,
            });
            let name = name.unwrap_or_default().into();
            Action::HandBack(id, name)
        }
        transcript::Action::Unfold(entry) => Action::Unfold(id, entry),
        transcript::Action::Toggle(entry) => {
            let child = app
                .agent_children(&id)
                .find(|a| a.launched == Some(entry))
                .map(|a| a.session.clone());
            child.map_or(Action::Call(id, entry), Action::Child)
        }
    }
}

const HELD: &str = " · Quitting puts the unanswered words in the draft. If the server got them, the transcript shows them; send them again only if it doesn't.";
const NEWER: &str =
    "Newer server: close this and run `razor --continue`. Part of the transcript may be missing.";

// The band's rows under the notice: the new agents' words a quit would lose, the input left
// unanswered and, while a quit is armed, where its words go, then the voice note and player,
// each wrapped whole.
fn warned(app: &App, columns: usize) -> Vec<ratatui::text::Line<'static>> {
    let held = app.held.as_ref().map_or_else(Vec::new, |(why, _)| {
        let again = match app.quitting {
            true => span(HELD, WHITE),
            false => span(bindings::text(app, " · <composer.send> tries again"), GRAY),
        };
        inset(vec![span(format!("✗ {why}"), AMBER), again], columns)
    });
    let (unsent, audio) = (
        super::start::unsent(app, columns),
        super::audio::band(app, columns),
    );
    [unsent, held, audio].concat()
}

/// The band above the composer: a rule or the notice, the question, the inputs queued for the
/// next tool boundary (three rows at most), the image tray and the children strip. The rule
/// shows under a band, and always on a phone.
pub(super) fn stack(app: &mut App, b: &mut Buffer, r: Rect, now: u64, hits: &mut Vec<Hit>) {
    if r.height == 0 {
        return;
    }
    let (menu, queued) = (overlay(app), queued(app, now));
    let images = app.composer.active_images().next().is_some();
    let recap = super::recap::lines(app, r.width).len() as u16;
    let reply = app.transcript().is_some_and(|t| t.pin.is_some());
    let ruled = app.shown_ask.is_some() || menu > 0 || !queued.is_empty() || images || reply;
    let lamps = super::tools::lamps::rows(app.transcript()) > 0;
    let ruled = ruled || lamps || recap > 0 || app.columns < PHONE || unanswered(app).is_some();
    let rule = if ruled { "▁" } else { " " }.repeat(r.width as usize);
    put(b, r.x, r.y, &rule, r.width, style(GRAY_DIM, GLASS));
    notice(app, b, r, now);
    let strip = strip::rows(app, now);
    let below = r.bottom().saturating_sub(strip);
    let mut y = r.y + 1;
    let warned = warned(app, r.width.into());
    for row in warned.iter().take(below.saturating_sub(y).into()) {
        let ground = style(DATA, GLASS).patch(row.style);
        fill(b, Rect::new(r.x, y, r.width, 1), ground);
        line(b, r, y, row, ground);
        y += 1;
    }
    if let Some(t) = app.transcript() {
        let h = super::tools::lamps::rows(Some(t)).min(r.bottom().saturating_sub(y));
        super::tools::lamps::draw(app, t, b, Rect::new(r.x, y, r.width, h), now, hits);
        y += h;
    }
    y += pinned(
        app,
        b,
        Rect::new(r.x, y, r.width, below.saturating_sub(y)),
        hits,
    );
    let asked = app.shown_ask.as_ref().map(|q| {
        let h = questions::rows(app, q, r.width, b.area.height);
        let h = h.min(below.saturating_sub(y + queued.len() as u16));
        (h, questions::draw(app, b, Rect::new(r.x, y, r.width, h), q))
    });
    if let Some((h, drawn)) = asked {
        // The options drawn whole, which the digits answer and the editor's title counts.
        app.visible_options
            .extend(drawn.iter().filter_map(|(_, action)| match action {
                Action::Option(_, n) => Some(*n),
                _ => None,
            }));
        hits.extend(drawn);
        y += h;
    }
    let menu = Rect::new(r.x, y, r.width, menu.min(below.saturating_sub(y)));
    // A grid the band has no row for closes, as the model picker does, and gives the keys back.
    if app.grid.is_some() && menu.height == 0 {
        app.grid = None;
        app.say("There is no room for the emoji picker; nothing was applied.");
    }
    match () {
        _ if app.grid.is_some() => emoji::draw(app, b, menu, hits),
        _ if completion::rows(app) > 0 => completion::draw(app, b, menu, hits),
        _ => shortcodes::draw(app, b, menu, hits),
    }
    y += menu.height;
    let rows = Rect::new(r.x, y, r.width, below.saturating_sub(y));
    y += waiting(app, b, rows, &queued, now, hits);
    if let Some(words) = unanswered(app).filter(|_| y < below) {
        let (head, ink) = ("No reply to your message '", style(CYAN, RECESS));
        let room = (r.width as usize).saturating_sub(width(head) + 3);
        fill(b, Rect::new(r.x, y, r.width, 1), ink);
        let line = format!("{head}{}'", clip(words, room));
        put(b, r.x + 1, y, &line, r.width.saturating_sub(2), ink);
        y += 1;
    }
    let recap = recap.min(below.saturating_sub(y));
    let images = Rect::new(r.x, y, r.width, below.saturating_sub(y + recap));
    composer::draw_images(&app.composer, b, images, hits);
    super::recap::draw(app, b, Rect::new(r.x, below - recap, r.width, recap), now);
    strip::draw(app, b, Rect::new(r.x, below, r.width, strip), now, hits);
}

/// A notice on `r`'s first row, in capitals on the bezel: the quit a second Ctrl-C makes, the
/// latest notice, or a newer server's warning; nothing without one.
pub(super) fn notice(app: &App, b: &mut Buffer, r: Rect, now: u64) {
    let notice = app.notice.as_ref().filter(|(_, until)| now < *until);
    let notice = notice.map(|(sentence, _)| sentence.as_str());
    let then = match app.ui.quit {
        Quit::Detach => "quit. <composer.back> to stay. The conversation keeps running.",
        Quit::End => "end this session. <composer.back> to stay. /background leaves it running.",
    };
    let quit = super::bindings::text(app, &format!("Press <global.quit> again to {then}"));
    let said = app.quitting.then_some(quit.as_str()).or(notice);
    // Once any reader skipped a line, the person is told for the rest of the terminal's life,
    // as written: the command is lowercase.
    let newer = app.api.as_ref().is_some_and(super::api::Api::newer);
    let said = said.map(str::to_uppercase);
    if let Some(sentence) = said.or_else(|| newer.then(|| NEWER.into())) {
        fill(b, Rect::new(r.x, r.y, r.width, 1), style(DATA, BEZEL));
        let room = r.width.saturating_sub(2);
        let words = clip(&sentence, room.max(8) as usize);
        put(b, r.x + 1, r.y, &words, room, bold(WHITE, BEZEL));
    }
}

// The pinned reply's rows from `r`'s top, as many as fit, on the agent's ground: the first with
// its unpin label, the last with the rows left out; returns the rows drawn.
fn pinned(app: &App, b: &mut Buffer, r: Rect, hits: &mut Vec<Hit>) -> u16 {
    let Some((session, id, reply)) = pin(app, r.width) else {
        return 0;
    };
    let (ink, more) = (style(DATA, AGENT_BG), reply.total - reply.lines.len());
    let shown = reply.lines.len().min(r.height as usize);
    for (n, (words, y)) in reply.lines.iter().zip(r.y..).take(shown).enumerate() {
        let row = Rect::new(r.x, y, r.width, 1);
        fill(b, row, ink);
        put(b, r.x + 1, y, "▌", 1, style(VIOLET, AGENT_BG));
        line(b, Rect { x: r.x + 3, ..row }, y, &words.words, ink);
        hits.push((row, Action::Jump(session.clone(), id)));
        let label = match n + 1 == shown && more > 0 {
            _ if n == 0 => "✕ unpin ".to_owned(),
            true => format!("+{more} more "),
            false => String::new(),
        };
        let (w, tone) = (width(&label) as u16, [GRAY, VIOLET][usize::from(n == 0)]);
        let at = Rect::new(r.right().saturating_sub(w), y, w, 1);
        put(b, at.x, y, &label, w, style(tone, AGENT_BG));
        hits.push((at, Action::Pin(session.clone(), id)));
    }
    shown as u16
}

// The queued inputs' rows from `r`'s top, as many as fit; the newest offers the take-back, which
// a phone keyboard, having no Up key, names by its arrow. Returns the rows drawn.
fn waiting(
    app: &App,
    b: &mut Buffer,
    r: Rect,
    queued: &[Queued],
    now: u64,
    hits: &mut Vec<Hit>,
) -> u16 {
    let newest = queued.iter().rposition(|row| row.1.is_some());
    let arrow = ["↑", "↶"][usize::from(app.columns < PHONE)];
    let shown = queued.iter().zip(r.y..).take(r.height as usize);
    for (i, ((words, input, lit), y)) in shown.enumerate() {
        let row = Rect::new(r.x, y, r.width, 1);
        let (cw, ink) = (if newest == Some(i) { 12 } else { 0 }, style(GRAY, RECESS));
        let room = r.width.saturating_sub(2 + cw);
        fill(b, row, ink);
        put(b, r.x + 1, y, &clip(words, room as usize), room, ink);
        if cw > 0 && r.width > cw {
            let x = put(b, r.right() - cw, y, arrow, 1, style(CYAN, RECESS));
            put(b, x, y, " take back ", cw - 1, ink);
        }
        hits.extend(input.map(|input| (row, Action::TakeBack(input))));
        if *lit > 0. {
            let (fg, bg) = (mix(RECESS, CYAN, *lit), mix(RECESS, YOU_BG, *lit));
            tint(b, row, fg, bg, 1.);
            app.next.at(now);
        }
    }
    queued.len().min(r.height as usize) as u16
}

// The person's newest message, when a turn ended after it with no reply and the session is
// not working.
fn unanswered(app: &App) -> Option<&str> {
    let agent = app.agent_row(app.focus.as_ref()?);
    let working = agent.is_some_and(|a| a.state == State::Working);
    app.transcript()?.unanswered().filter(|_| !working)
}

// The addressed session's pinned reply, rendered as the transcript renders a reply, its first
// five rows (three on a phone).
fn pin(app: &App, width: u16) -> Option<(SessionId, EntryId, super::markdown::Reply)> {
    let (session, (id, text)) = (app.focus.clone()?, app.transcript()?.pin.as_ref()?);
    let rows = [5, 3][usize::from(app.columns < PHONE)];
    let reply =
        super::markdown::render(text, width.saturating_sub(13).into(), false, None, 0..rows);
    Some((session, *id, reply))
}

// The band's one overlay: the emoji grid, the completion list or the shortcode strip.
fn overlay(app: &App) -> u16 {
    match (emoji::rows(app), completion::rows(app)) {
        (0, 0) => shortcodes::rows(app),
        (0, menu) => menu,
        (grid, _) => grid,
    }
}

/// A queued row: its words, the input a press takes back, and how lit it is.
type Queued = (String, Option<EntryId>, f64);

// The band's queued inputs: one row per input, its first line, three at most with the rest
// counted on the last; and while motion plays, the input that just left, lit where it was (last
// when taken back, first when the model took it).
fn queued(app: &App, now: u64) -> Vec<Queued> {
    let Some(t) = app.transcript() else {
        return Vec::new();
    };
    let words = |id: &EntryId| match t.entries.get(id).map(|e| &e.body) {
        Some(Body::Input(Input {
            command: Some(c), ..
        })) => format!("↳ queued · {} · after this turn", c.typed()),
        Some(Body::Input(input)) => {
            let mut lines = input.text.split('\n');
            let first = lines.next().unwrap_or("").trim();
            let more = if lines.next().is_some() { " …" } else { "" };
            let note = ["", "▶ voice note · "][usize::from(input.voice.is_some())];
            format!("↳ queued · {note}{first}{more}")
        }
        _ => "↳ queued · waiting for the next tool boundary".into(),
    };
    let ids: Vec<_> = t.queued.difference(&t.taking).copied().collect();
    let mut rows: Vec<_> = ids
        .iter()
        .take(3)
        .map(|id| (words(id), Some(*id), 0.))
        .collect();
    if ids.len() > 3 {
        let more = format!("↳ +{} more queued", ids.len() - 2);
        rows.truncate(2);
        rows.push((more, ids.last().copied(), 0.));
    }
    if let Some((input, at, back)) = t.left.filter(|l| motion::on(app) && now < l.1 + 160) {
        let lit = 1. - motion::ease_out(motion::progress(now, at, 160));
        let at = if back { rows.len() } else { 0 };
        rows.insert(at, (words(&input), None, lit));
    }
    rows
}

// A status pane occupying the right side keeps notices on its last row.
fn status_side(app: &mut App, b: &mut Buffer, r: Rect, now: u64, hits: &mut Vec<Hit>) {
    status::draw(app, b, r, now, hits);
    notice(
        app,
        b,
        Rect::new(r.x, r.bottom().saturating_sub(1), r.width, 1),
        now,
    );
}
