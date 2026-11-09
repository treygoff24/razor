//! Clicks, drags and the wheel, and what a drawn target does when it is clicked or chosen with
//! Enter: a click target is its drawn area and action, and the last drawn wins.
use super::keys::{CUT, focus, fresh, leaving, remove_image};
use super::transcript::select;
use super::{App, Transcript, bindings, clipboard, composer, mcp, picker, view::Action};
use crate::protocol::{Answer, Body, EntryId, SessionId, now};
use composer::{completion, emoji, shortcodes};
use crossterm::event::{KeyModifiers as Mods, MouseButton, MouseEvent, MouseEventKind};
use reqwest::Method;

/// One mouse event. An open panel takes its own clicks, the new-agent chooser only its rows', and
/// any other modal surface none.
pub fn mouse(app: &mut App, m: MouseEvent, modal: bool) {
    let at = (m.column, m.row).into();
    let action = app
        .hits
        .iter()
        .rev()
        .find(|(r, _)| r.contains(at))
        .map(|(_, a)| a.clone());
    if app.panel.is_some() {
        if m.kind == MouseEventKind::Down(MouseButton::Left) {
            match action {
                Some(a @ Action::Task(_)) => act(app, a),
                other => mcp::click(app, other),
            }
        }
        return;
    }
    if app.start.is_some() {
        if let (Some(a @ Action::Choose(..)), MouseEventKind::Down(_)) = (action, m.kind) {
            act(app, a)
        }
        return;
    }
    if modal {
        return;
    }
    if let MouseEventKind::ScrollUp | MouseEventKind::ScrollDown = m.kind {
        let down = m.kind == MouseEventKind::ScrollDown;
        let step = if down { 3 } else { -3 };
        app.selection = None;
        if let Some(scroll) = &mut app.list.overview {
            *scroll = scroll.saturating_add_signed(step);
        } else if app.list.area.is_some_and(|r| r.contains(at)) {
            app.list.scroll = app.list.scroll.saturating_add_signed(step);
        } else if app.status_area.is_some_and(|r| r.contains(at)) {
            app.status_scroll = app.status_scroll.saturating_add_signed(step);
        } else if let Some(t) = wheeled(app, at) {
            // One row per event: a terminal sends several for each notch of the wheel.
            match down {
                true => t.page_down(1),
                false => t.page_up(1),
            }
        }
        return;
    }
    let cell = (m.column, m.row);
    // A held list edge follows the mouse until it lets go, which keeps the width.
    if app.list.dragging {
        let left = app.list.area.map_or(0, |a| a.x);
        let columns = (m.column + 1).saturating_sub(left);
        match m.kind {
            MouseEventKind::Drag(MouseButton::Left) => super::agents::resize(app, columns, false),
            MouseEventKind::Up(MouseButton::Left) => super::agents::resize(app, columns, true),
            _ => {}
        }
        return;
    }
    if app.list.overview.is_none() && draft(app, m, cell) {
        app.list.keys = false;
        return;
    }
    match m.kind {
        MouseEventKind::Drag(MouseButton::Left) => return select::drag(app, cell),
        MouseEventKind::Up(MouseButton::Left) => {
            return select::release(app, cell, now())
                .into_iter()
                .for_each(|a| act(app, a));
        }
        _ => {}
    }
    let grid = matches!(action, Some(Action::Grid(_) | Action::Emoji));
    if let (Some(_), MouseEventKind::Down(_), false) = (&app.grid, m.kind, grid) {
        app.grid = None;
    }
    if let (Some(Action::Grid(h)), MouseEventKind::Down(MouseButton::Right)) = (&action, m.kind) {
        return emoji::click(app, *h, true);
    }
    if m.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    if !select::press(app, cell, action.clone()) {
        action.into_iter().for_each(|a| act(app, a));
    }
}

// The transcript the wheel turns: a split's main half when this frame drew it under the pointer,
// else the addressed one, its opened card included.
fn wheeled(app: &mut App, at: ratatui::layout::Position) -> Option<&mut Transcript> {
    let root = app.root.clone().filter(|_| app.split());
    match root.filter(|id| app.transcripts.get(id).is_some_and(|t| t.holds(at))) {
        Some(id) => app.transcripts.get_mut(&id),
        None => app.transcript_mut(),
    }
}

// The mouse in the draft, as the cockpit's: a press puts the cursor there, a drag selects, Shift
// extends, a double click takes the word, and a selection the mouse made is copied on release.
// True when the draft took the event.
fn draft(app: &mut App, m: MouseEvent, cell: (u16, u16)) -> bool {
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let at = now();
            let double = matches!(app.clicks, (last, c, 1) if c == cell && at < last + 500);
            // Anything drawn over the words with a click of its own, such as the emoji grid, keeps it.
            let covered = app.hits.iter().any(|(r, _)| r.contains(cell.into()));
            let shift = m.modifiers.contains(Mods::SHIFT);
            if covered || !app.composer.press(cell, shift, double) {
                return false;
            }
            // A third press on the cell starts over rather than counting as another double; a
            // press in the words lets go of a chosen image chip.
            (app.clicks, app.selection) = ((at, cell, u8::from(!double)), None);
            app.composer.chosen = None;
        }
        MouseEventKind::Drag(MouseButton::Left) => return app.composer.drag(cell),
        MouseEventKind::Up(MouseButton::Left) => match app.composer.release() {
            Some(Some(words)) => clipboard::draft(app, words),
            Some(None) => {}
            None => return false,
        },
        _ => return false,
    }
    true
}

/// What a click on a target does, or Enter on the selected one.
pub(super) fn act(app: &mut App, action: Action) {
    if leaving(app) {
        return;
    }
    match action {
        Action::Record => super::audio::record(app),
        Action::Edge => app.list.dragging = true,
        Action::Play(session, entry) => super::audio::play(app, session, entry),
        Action::Child(id) => focus(app, Some(id)),
        Action::Option(_, _) if fresh(app) => {}
        Action::Option(id, n) => super::inputs::admit_answer(app, id, Answer::Option(n)),
        Action::Cut => app.say(CUT),
        Action::Call(session, id) => super::transcript::jump(app, &session, id, true),
        Action::Chip(at) => app.composer.chosen = Some(at),
        Action::Unfold(session, id) => held(app, &session, |t| t.expand([id], true)),
        Action::Back => focus(app, None),
        Action::Overview => super::agents::overview(app),
        Action::Close => super::agents::toggle(app),
        Action::Archive => app.archive = !app.archive,
        a @ (Action::Section(_)
        | Action::Earlier
        | Action::Agent(_)
        | Action::New
        | Action::Thread(_)) => super::agents::press(app, a),
        Action::Choose(server, cwd) => super::start::choose(app, Some((server, cwd))),
        Action::Inbox(offset) => {
            (app.inbox, app.pane_open, app.to_inbox) = (offset, Some(true), true)
        }
        Action::Tasks(offset) => app.tasks.offset = offset,
        Action::Task(id) => super::tasks::detail(app, id),
        Action::Delivery(session, entry, _) => delivery(app, session, entry),
        Action::Strip => app.strip = !app.strip,
        Action::Complete(n) => completion::click(app, n),
        Action::Emoji => emoji::open(app, shortcodes::typed(app)),
        Action::Grid(h) => emoji::click(app, h, false),
        Action::Code(n) => shortcodes::click(app, n),
        Action::Model => picker::open(app, false),
        Action::Image => app.say("Paste or drop an image's file path to attach it."),
        Action::Send if app.held.is_some() => super::keys::enter(app),
        Action::Send if app.composer.text.trim().is_empty() => app.say("Nothing to send yet."),
        Action::Send => super::keys::enter(app),
        Action::HandBack(session, name) => super::inputs::hand_back(app, session, name),
        Action::Status => {
            // Beside the agents list the pane is closed until asked for.
            let wide = app.columns >= 140 && app.list.area.is_none();
            app.pane_open = Some(!app.pane_open.unwrap_or(wide));
        }
        Action::Copy(text) => clipboard::copy(app, Some(text)),
        Action::CopyText(session, entry) => match reply(app, &session, entry) {
            Some(text) => clipboard::copy(app, Some(text)),
            None => app.say(GONE),
        },
        Action::RemoveImage(number) => remove_image(app, number),
        Action::TakeBack(input) => super::inputs::take_back(app, input),
        Action::Quote => select::quote(app),
        Action::Jump(session, id) => {
            super::transcript::jump(app, &session, id, false);
            app.say(&bindings::text(
                app,
                "Jumped in transcript · <transcript.bottom> for latest.",
            ));
        }
        Action::Latest(session) => held(app, &session, Transcript::bottom),
        Action::Pin(session, id) => held(app, &session, |t| t.pin(Some(id))),
        // Only an open `/mcp` draws these, and it takes its own clicks.
        Action::Mcp(_) => {}
        Action::Link(url) => clipboard::copy(app, Some(url.to_string())),
        Action::Ask(id) => {
            let n = app.shown_ask.as_ref().map_or(0, |a| a.options.len());
            (app.ask_open, app.ask_at) = (Some(id), now());
            app.say(&format!("Press 1-{n} to answer."));
        }
    }
}

pub(super) const GONE: &str =
    "That reply is no longer in the transcript buffer; use recall to copy it.";

/// The words of a reply the transcript still holds.
pub(super) fn reply(app: &App, session: &SessionId, entry: EntryId) -> Option<String> {
    let entry = app.transcripts.get(session)?.entries.get(&entry)?;
    match &entry.body {
        Body::Text { text } | Body::Thinking { text } => Some(text.clone()),
        _ => None,
    }
}

// A delivery opens as a link does, its address, its path or a note's line copied; the copy
// starting marks it read.
fn delivery(app: &mut App, session: SessionId, entry: EntryId) {
    if app.copying {
        return app.say("A clipboard write is pending; wait before opening another delivery.");
    }
    let Some(delivery) = app
        .delivered
        .iter()
        .find(|d| (&d.session, d.entry) == (&session, entry))
    else {
        return;
    };
    let Some(target) = delivery
        .url
        .clone()
        .or_else(|| delivery.path.as_ref().map(|p| p.display().to_string()))
        // A note, with neither file nor url, opens to its line as a link does to its address.
        .or_else(|| delivery.file.is_none().then(|| delivery.line.clone()))
    else {
        return app.say("The delivery has no opening path; reconnect to its owning server.");
    };
    let read = delivery.read;
    clipboard::copy(app, Some(target));
    if !read && app.copying {
        let path = format!("v1/sessions/{session}/deliveries/{}/read", entry.0);
        app.write(|api| async move {
            let result = api.json(Method::PUT, &path, None).await;
            move |app: &mut App| result.err().iter().for_each(|e| app.say(&e.to_string()))
        });
    }
}

// Acts on `session`'s transcript while the app holds it.
fn held(app: &mut App, session: &SessionId, act: impl FnOnce(&mut Transcript)) {
    if let Some(t) = app.transcripts.get_mut(session) {
        act(t);
    }
}
