//! Keys and pastes act on the app directly; their API work is spawned beside the loop or queued
//! behind earlier writes, and its answer is applied between draws.
use super::clicks::{self, GONE, act, reply};
use super::inputs::{answer, history, send, take_back};
use super::transcript::{Direction, select};
use super::{App, activity, bindings, clipboard, composer, data, panel, picker};
use super::{editor::Editing, recap, view::Action};
use crate::protocol::{Answer, EntryId, SessionId, now};
use composer::{completion, emoji, shortcodes};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers as Mods};
use reqwest::Method;

/// One terminal event; true for a second Ctrl-C in a row, which quits without stopping a session
/// or its jobs, unless `[ui] quit` is `end`: then the addressed session is ended first. An open
/// picker, panel or new-agent chooser takes every key and drops pastes, as the emoji grid drops
/// them, and only a panel's or the chooser's own rows take clicks.
pub fn event(app: &mut App, event: super::tty::Event) -> bool {
    let event = match event {
        super::tty::Event::Input(event) => event,
        super::tty::Event::Picture(text) => {
            app.pictures.answer(&text);
            app.dirty = true;
            return false;
        }
    };
    if let Event::Key(k) = &event
        && app.program.answer(k)
    {
        app.dirty = true;
        return false;
    }
    (app.dirty, app.immediate) = (true, true);
    let modal =
        app.picker.is_some() || app.panel.is_some() || app.start.is_some() || app.goto.is_some();
    match event {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            activity::report(app);
            let Some((k, context, action)) = bindings::translate(app, k) else {
                return false;
            };
            let quits = action == "global.quit";
            if app.quitting && quits {
                if app.ui.quit == crate::protocol::Quit::End {
                    super::commands::done(app, "quit");
                }
                return true;
            }
            // The stage hears each key, and a long draft any key but Enter empties.
            let long = app.composer.text.chars().nth(39).is_some();
            key(app, k, context, action, quits);
            let cleared = long && app.composer.text.is_empty();
            app.stage.key(now(), cleared && k.code != KeyCode::Enter);
            return false;
        }
        Event::FocusLost => {
            recap::leave(app);
            app.presence.report(false, now());
            return false;
        }
        Event::FocusGained if !app.presence.focused => {
            app.presence.report(true, now());
            recap::back(app);
            recap::check(app);
        }
        Event::Paste(text) => {
            activity::report(app);
            app.quitting = false;
            // Words pasted into the draft leave browsing, as typed ones do.
            if !modal && app.grid.is_none() && app.list.overview.is_none() {
                app.dismiss_suggestion();
                app.unselect();
                composer::paste(app, text);
            }
        }
        Event::Mouse(m) => {
            activity::report(app);
            clicks::mouse(app, m, modal);
        }
        _ => {}
    }
    // Focus, a click, a scroll or a paste wakes a resting room as a key does.
    app.stage.key(now(), false);
    false
}

/// Applies one resolved key with the binding table's explicit context and action.
fn key(app: &mut App, k: KeyEvent, context: &str, action: &str, quits: bool) {
    // Ctrl-C arms quitting; while armed, Esc only disarms and any other key also does its own work.
    let armed = std::mem::replace(&mut app.quitting, quits);
    if app.quitting || armed && k.code == KeyCode::Esc {
        return;
    }
    let editing = matches!(context, "text" | "editor")
        || matches!(action, "composer.clear" | "global.copy" | "global.cut");
    if !editing && leaving(app) {
        return;
    }
    if action == "global.overview" {
        return super::agents::overview(app);
    }
    // The overview takes the list's keys and nothing else.
    if app.list.overview.is_some() && context != "list" {
        return;
    }
    if (context == "voice" || action == "global.record") && super::audio::key(app, k) {
        return;
    }
    (app.selection, app.recap) = (None, None);
    app.transcript_mut()
        .into_iter()
        .for_each(|t| t.ended = None);
    if app.goto.is_some() {
        return super::goto::key(app, k, action);
    }
    if app.panel.is_some() {
        return if matches!(context, "panel" | "mcp") {
            panel::key(app, k)
        };
    }
    // PageUp and PageDown still scroll the transcript behind the picker.
    if app.picker.is_some() && !matches!(k.code, KeyCode::PageUp | KeyCode::PageDown) {
        return if context == "picker" {
            picker::key(app, k)
        };
    }
    if context == "prefix" {
        return prefixed(app, action);
    }
    if action == "global.new" {
        return super::start::open(app);
    }
    // The keys move to the list with the addressed agent selected, or back to the conversation.
    if action == "composer.switch" {
        return match app.list.open && app.list.keys {
            true => app.list.keys = false,
            false => super::agents::choose(app),
        };
    }
    if app.start.is_some() {
        return super::start::key(app, k);
    }
    if context == "list" {
        return super::agents::key(app, action);
    }
    if app.grid.is_some() {
        return emoji::key(app, k);
    }
    if action == "global.emoji" {
        return emoji::open(app, shortcodes::typed(app));
    }
    // A question closed while its answer was being written: every key says so and the words stay
    // a draft. Only the Enter or Esc meant for the answer ends it, doing nothing else, so words
    // typed after the close never leave as an input.
    let open = |id: &super::MailId| app.asks().iter().any(|a| &a.id == id);
    if app.sending.is_none() && app.answering.as_ref().is_some_and(|id| !open(id)) {
        app.say("That question closed; your words stay in the draft.");
        if matches!(k.code, KeyCode::Enter | KeyCode::Esc) {
            app.answering = None;
            return;
        }
    }
    if matches!(action, "global.copy" | "global.cut") {
        return copy(app, action == "global.cut");
    }
    if matches!(action, "composer.clear" | "composer.stash") {
        return aside(app, action == "composer.stash");
    }
    if context != "attachments" {
        app.composer.chosen = None;
    }
    match context {
        "suggestion" => app.accept_suggestion(),
        "attachments" => attachments(app, k),
        "completion" if completion::key(app, k) => {}
        "shortcode" if shortcodes::key(app, k) => {}
        "completion" | "shortcode" => {
            if k.code == KeyCode::Enter {
                enter(app)
            } else if app.composer.key(k) {
                app.edited();
            }
        }
        "global" | "transcript" | "skill" | "children" => browse(app, k),
        _ => edit(app, k, context, action),
    }
}

// One prefix action, by its name in the key table; the prefix armed only with an empty draft, a
// command it types takes no words.
fn prefixed(app: &mut App, action: &str) {
    let typed = |app: &mut App, command: &str| {
        app.composer.replace(command.into());
        app.list.keys = false;
        app.edited();
    };
    match action.trim_start_matches("prefix.") {
        "rename" => typed(app, "/name "),
        "done" => typed(app, "/done"),
        "split" => {
            let first = kin(app, app.focus.clone()).into_iter().next();
            match app.selected_child().cloned().or(first) {
                Some(id) => focus(app, Some(id)),
                None => app.say("No children out."),
            }
        }
        "close_split" if app.split() => focus(app, None),
        "close_split" => app.say("No child's split is open."),
        "right" => app.list.keys = false,
        "switch" if app.list.open && app.list.keys => app.list.keys = false,
        "left" | "switch" | "wider" | "narrower" if !app.list.open => super::agents::toggle(app),
        "left" | "switch" => app.list.keys = true,
        "wider" | "narrower" => super::agents::widen(app, action == "prefix.wider"),
        // The oldest question waiting for the person, which the status chip names; else what a
        // press on the chip opens.
        "waiting" => {
            let asks = app.agents.values().flat_map(|a| &a.asks);
            match asks.min_by_key(|q| (q.at, &q.id)).map(|q| q.id.clone()) {
                Some(ask) => {
                    app.ask = Some(ask.clone());
                    super::agents::open(app, ask.0);
                }
                None => act(app, super::status::press(app)),
            }
        }
        "keys" => _ = panel::command(app, "/keys"),
        "detach" => app.leaving = true,
        "browse" => select(app, true, false),
        // The ring runs newest first, so the reply above is the next one along it.
        "previous_reply" | "next_reply" => select(app, action == "prefix.previous_reply", true),
        "goto" => super::goto::open(app),
        "cancel" => {}
        jump => super::agents::jump(app, jump),
    }
}

// The clear key empties the draft as one undo step. The stash key sets the draft aside, words and
// images, or with an empty draft brings it back; one is held at a time, never overwritten.
fn aside(app: &mut App, stash: bool) {
    match (stash, app.composer.is_empty(), app.stash.is_some()) {
        (false, ..) => _ = app.composer.swap(0..app.composer.text.len(), ""),
        (true, false, false) => app.stash = Some(std::mem::take(&mut app.composer)),
        (true, false, true) => {
            let held =
                "A draft is already stashed; <composer.stash> with an empty draft brings it back.";
            return app.say(&bindings::text(app, held));
        }
        (true, true, _) => match app.stash.take() {
            Some(stashed) => app.composer = stashed,
            None => return app.say("No draft is stashed."),
        },
    }
    app.edited();
}

fn copy(app: &mut App, cut: bool) {
    if app.copying {
        return app.say("A clipboard write is pending; wait before copying or cutting again.");
    }
    if !cut && app.composer.text.is_empty() {
        if let Some(text) = super::audio::failure(app) {
            return clipboard::copy(app, Some(text.to_owned()));
        }
        if app.shown_ask.is_some() {
            return clipboard::question(app);
        }
        if let Some((text, _)) = app.notice.as_ref().filter(|(_, until)| now() < *until) {
            return clipboard::copy(app, Some(text.clone()));
        }
    }
    // A cut's words leave only when its copy can start.
    if cut && app.pending >= 8 {
        return app.say("Eight API operations are pending; wait for an answer before cutting.");
    }
    let Some(text) = app.composer.copy_selection(cut) else {
        return app.say("Nothing is selected to cut.");
    };
    if cut {
        app.edited();
    }
    clipboard::draft(app, text);
}

fn browse(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(Mods::CONTROL);
    match k.code {
        KeyCode::Esc => escape(app),
        KeyCode::Char('x') if ctrl => stop(app),
        KeyCode::Char(c @ ('n' | 'p')) if ctrl && app.composer.text.is_empty() => {
            children(app, c == 'n')
        }
        KeyCode::PageUp | KeyCode::PageDown | KeyCode::Char('u' | 'd') => {
            if let Some(t) = app.transcript_mut() {
                // Ctrl-U and Ctrl-D scroll half as far.
                let page = (t.detail_height.saturating_sub(2).max(3) as usize) >> u8::from(ctrl);
                match matches!(k.code, KeyCode::PageUp | KeyCode::Char('u')) {
                    true => t.page_up(page),
                    false => t.page_down(page),
                }
            }
        }
        KeyCode::Char('t') if ctrl => act(app, Action::Status),
        // Alt and a digit opens that lamp's skill card, never typing the digit.
        KeyCode::Char(c @ '1'..='9') if k.modifiers == Mods::ALT => {
            let slot = usize::from(c as u8 - b'1');
            let t = app.transcript_mut();
            match t
                .as_ref()
                .and_then(|t| super::tools::lamps::slot_entry(t, slot))
            {
                Some(id) => t.into_iter().for_each(|t| t.toggle(id)),
                None => app.say(&format!("No skill is lit in slot {c}.")),
            }
        }
        KeyCode::Char('o') if ctrl => {
            if let Some(said) = app.transcript_mut().map(|t| t.open_all()) {
                app.say(said);
            }
        }
        KeyCode::Home if ctrl => {
            if let Some(id) = app.focus.clone() {
                super::transcript::jump(app, &id, EntryId(1));
            }
        }
        KeyCode::End if ctrl => {
            if let Some(t) = app.transcript_mut() {
                match t.opened {
                    Some(_) => t.detail_scroll = usize::MAX,
                    None => t.bottom(),
                }
            }
        }
        _ => {}
    }
}

// Esc backs out one layer: the selected child card, the opened card, the answer, the split view, a
// floating status pane, the pinned reply, an offered next message, and only then stops the session
// (Trey, 2026-10-30).
fn escape(app: &mut App) {
    if app.unselect() {
        return;
    }
    let opened = app
        .transcript_mut()
        .is_some_and(|t| t.opened.take().is_some());
    let left = !opened && app.answering.take().is_some();
    if left && !app.composer.text.trim().is_empty() {
        app.say("Left the answer; its words stay in the draft.");
    }
    if opened || left {
        return;
    }
    if app.split_view() {
        return focus(app, None);
    }
    if app.pane_open == Some(true) && !super::view::docked(app) {
        app.pane_open = Some(false);
        return;
    }
    match app.transcript_mut().filter(|t| t.pin.is_some()) {
        Some(t) => t.pin = None,
        // A new agent's composer goes back to the session addressed before it.
        None => match app.back.take().filter(|_| app.focus.is_none()) {
            Some(id) => super::agents::press(app, Action::Agent(id)),
            None if app.suggestion().is_some() => app.dismiss_suggestion(),
            None => stop(app),
        },
    }
}

// Ctrl-N and Ctrl-P: in a child's split, the next or previous sibling's split; on a phone, the
// next child's split at once; else, beside the list too, the next child card, the pane opened
// for it.
fn children(app: &mut App, forward: bool) {
    let split = app.split_view();
    let parent = match split {
        true => app.row().and_then(|a| a.parent.clone()),
        false => app.focus.clone(),
    };
    let kin = kin(app, parent);
    let n = kin.len();
    if n < 1 + usize::from(split) {
        return app.say(["No children out.", "No other child at this level."][usize::from(split)]);
    }
    let here = if split {
        app.focus.clone()
    } else {
        app.selected_child().cloned()
    };
    let at = kin.iter().position(|id| Some(id) == here.as_ref());
    let next = kin[step(at, n, forward)].clone();
    if split || app.columns < super::view::PHONE {
        return focus(app, Some(next));
    }
    // The selection is the next child of the list, whether or not its card is drawn yet.
    (app.pane_open, app.selected) = (Some(true), Some(Action::Child(next)));
    app.revision = app.revision.wrapping_add(1);
}

// The children of `parent` in launch order.
fn kin(app: &App, parent: Option<SessionId>) -> Vec<SessionId> {
    let mut kin: Vec<_> = (app.agents.values())
        .filter(|a| parent.is_some() && a.parent == parent)
        .map(|a| (a.launched, a.session.clone()))
        .collect();
    kin.sort();
    kin.into_iter().map(|(_, id)| id).collect()
}

// The place after `at` round a ring of `n`, or before it; with none, the first or the last.
fn step(at: Option<usize>, n: usize, forward: bool) -> usize {
    match forward {
        true => at.map_or(0, |i| (i + 1) % n),
        false => at.map_or(n - 1, |i| (i + n - 1) % n),
    }
}

// The browse ring: every drawn click target in the ring's order, or one stop for each reply;
// Enter on one does what a click does, or reads a reply.
fn select(app: &mut App, forward: bool, replies: bool) {
    let mut ring = app.targets.clone();
    // A reply's stops, its heading and a long one's copy line, lie together on screen.
    if replies {
        ring.retain(|t| replied(t).is_some());
        ring.dedup_by(|a, b| replied(a) == replied(b));
    }
    let n = ring.len();
    if n == 0 {
        return app.say(["nothing to open yet", "No reply is on screen."][usize::from(replies)]);
    }
    let same = |t: &Action, a: &Action| t == a || replies && replied(t) == replied(a);
    let at = (app.selected.as_ref()).and_then(|a| ring.iter().position(|t| same(t, a)));
    app.selected = Some(ring[step(at, n, forward)].clone());
    app.revision = app.revision.wrapping_add(1);
}

/// The reply a browse stop stands for: its heading row, or a long one's copy line.
pub(super) fn replied(a: &Action) -> Option<(&SessionId, EntryId)> {
    match a {
        Action::Pin(session, entry) | Action::CopyText(session, entry) => Some((session, *entry)),
        _ => None,
    }
}

fn edit(app: &mut App, k: KeyEvent, context: &str, action: &str) {
    app.list.keys = false;
    match context {
        // A digit past the ask's options is the draft's.
        "question" => match k.code {
            KeyCode::Char(n @ '1'..='9') => {
                let folded = app.hits.iter().find(|(_, a)| matches!(a, Action::Ask(_)));
                if let Some((_, open)) = folded.cloned() {
                    return act(app, open);
                }
                let option = n as usize - '1' as usize;
                match app.visible_options.contains(&option) {
                    _ if fresh(app) => {}
                    true => answer(app, Answer::Option(option)),
                    false => app.say(CUT),
                }
            }
            // Round the asks from the shown one, the first while none is held.
            KeyCode::Left | KeyCode::Right => {
                let ids: Vec<_> = app.asks().iter().map(|a| a.id.clone()).collect();
                let at = ids.iter().position(|id| Some(id) == app.ask.as_ref());
                let next = |n| step(Some(at.unwrap_or(0)), n, k.code == KeyCode::Right);
                app.ask = (!ids.is_empty()).then(|| ids[next(ids.len())].clone());
            }
            _ => {}
        },
        "line_start" => super::agents::outward(app, action),
        "draft_end" => super::agents::toggle(app),
        // The ring runs newest first, so Up steps along it, to older entries higher on screen.
        "targets" => match (k.code, app.selected.clone()) {
            (KeyCode::Char('"' | 'y') | KeyCode::Enter, Some(a)) if replied(&a).is_some() => {
                let text = replied(&a).and_then(|(session, entry)| reply(app, session, entry));
                match (text, k.code) {
                    (None, _) => app.say(GONE),
                    (Some(text), KeyCode::Enter) => app.editing = Some(Editing::Reply(text)),
                    (Some(text), KeyCode::Char('"')) => select::quoted(app, &text),
                    (Some(text), _) => clipboard::copy(app, Some(text)),
                }
            }
            (KeyCode::Char('y'), Some(a @ (Action::Link(_) | Action::Copy(_)))) => act(app, a),
            (KeyCode::Char('"' | 'y'), _) => app.say("Select a reply, link or copy line first."),
            _ => select(app, matches!(k.code, KeyCode::Tab | KeyCode::Up), false),
        },
        "input_history" => history(app, k.code == KeyCode::Up),
        "queued_input" => {
            let t = app.transcript();
            let newest = t.and_then(|t| t.queued.difference(&t.taking).last().copied());
            newest.into_iter().for_each(|input| take_back(app, input));
        }
        "attached_images" => app.composer.chosen = Some(app.composer.active_images().count() - 1),
        "composer" if k.code == KeyCode::Enter => enter(app),
        "composer" => escape(app),
        "editor" | "text" => {
            let chip = match k.code {
                KeyCode::Backspace => app.composer.image_by_cursor(false),
                KeyCode::Delete => app.composer.image_by_cursor(true),
                _ => None,
            };
            if context == "editor"
                && k.modifiers.is_empty()
                && let Some(number) = chip
            {
                return remove_image(app, number);
            }
            if app.composer.key(k) {
                app.unselect();
                app.edited();
            }
        }
        _ => {}
    }
}

pub(super) fn enter(app: &mut App) {
    // A trailing backslash is a new line: the phone's, which has no Shift-Enter.
    let end = app.composer.text.len();
    if app.composer.cursor() == end && app.composer.text.ends_with('\\') {
        app.composer.swap(end - 1..end, "\n");
        return app.edited();
    }
    if app.sending.is_some() {
        return app.say(&super::bindings::text(
            app,
            "The earlier input is still being admitted; press <composer.send> once it answers.",
        ));
    }
    let empty = app.composer.text.is_empty();
    if let (true, Some(target)) = (empty, app.selected.take()) {
        return act(app, target);
    }
    if app.answering.is_some() && !app.composer.text.trim().is_empty() {
        return answer(app, Answer::Text(app.composer.text.clone()));
    }
    if let (true, Some(ask)) = (empty, app.shown_ask.as_ref().filter(|_| !app.split_view())) {
        app.answering = Some(ask.id.clone());
    } else if app.composer.text.starts_with('/')
        && completion::skill(app, &app.composer.text).is_none()
    {
        super::commands::run(app);
    } else {
        let text = data::expand(&app.composer.text, &app.emoji);
        app.composer.replace(text);
        send(app);
    }
}

pub(super) const CUT: &str =
    "That option is not shown in full; answer its words with /answer instead.";

/// A question that just took the band is read before a digit or click can answer it.
pub(super) fn fresh(app: &mut App) -> bool {
    let fresh = now() < app.ask_at.saturating_add(600);
    if fresh {
        app.say("New question: read it first.");
    }
    fresh
}

// With an image chip chosen: Left and Right move along the chips, Backspace or Delete removes it
// and chooses its neighbour, Esc or Down lets go, and any other key lets go and is typed. True
// when the tray took the key.
fn attachments(app: &mut App, k: KeyEvent) {
    let n = app.composer.active_images().count();
    let Some(at) = app.composer.chosen.take().filter(|&at| at < n) else {
        return;
    };
    let chosen = match k.code {
        KeyCode::Left => Some((at + n - 1) % n),
        KeyCode::Right => Some((at + 1) % n),
        KeyCode::Backspace | KeyCode::Delete => {
            let number = app.composer.active_images().nth(at).map(|im| im.number);
            number
                .into_iter()
                .for_each(|number| remove_image(app, number));
            (n > 1).then(|| at.min(n - 2))
        }
        KeyCode::Esc | KeyCode::Down => None,
        _ => return,
    };
    app.composer.chosen = chosen;
}

pub(super) fn remove_image(app: &mut App, number: usize) {
    if app.sending.is_none() && app.composer.remove_image(number) {
        app.edits += 1;
        app.save(None);
    }
}

fn stop(app: &mut App) {
    if let Some(id) = app.focus.clone() {
        app.spawn(|api| async move {
            let stopped = api
                .json(Method::POST, &format!("v1/sessions/{id}/stop"), None)
                .await;
            move |app: &mut App| stopped.err().iter().for_each(|e| app.say(&e.to_string()))
        });
    }
}

/// While a switch waits for its save, only editing and Ctrl-C reach the app: a send, answer,
/// recording, move or list change would act for the session being left.
pub(super) fn leaving(app: &mut App) -> bool {
    let leaving = app.parting.is_some();
    if leaving {
        app.say("Saving before leaving; wait or Ctrl-C.");
    }
    leaving
}

/// Addresses `next`, or the addressed child's parent, once the server holds the old draft.
pub(super) fn focus(app: &mut App, next: Option<SessionId>) {
    if next.is_some() && next == app.focus || next.is_none() && !app.split() {
        return;
    }
    if app.waiting() {
        return app.say("A note, input or draft is pending; wait to change the addressed session.");
    }
    if super::start::room(app, None) {
        app.save(Some(Box::new(|app: &mut App| refocus(app, next))));
    }
}

fn refocus(app: &mut App, next: Option<SessionId>) {
    recap::leave(app);
    let old = app.focus.clone();
    if let (true, Some(old)) = (app.split(), app.focus.clone()) {
        app.streams
            .remove(&old)
            .iter()
            .for_each(|task| task.abort());
        // The child's map goes with it; one left paged into older history becomes a
        // newest-entry card again.
        if let Some(t) = app.transcripts.get_mut(&old) {
            t.marks = None;
            if t.retain_older {
                t.prepare_page(Direction::Newest);
                app.previews.remove(&old);
            }
        }
    }
    app.focus = match next {
        Some(id) => {
            app.root.get_or_insert(id.clone());
            app.status_scroll = 0;
            Some(id)
        }
        None => {
            let parent = app
                .focus
                .as_ref()
                .and_then(|id| app.agents.get(id)?.parent.clone());
            parent.or_else(|| app.root.clone())
        }
    };
    (app.selected, app.panel, app.history, app.initial) = (None, None, None, None);
    app.revision = app.revision.wrapping_add(1);
    // A session's words were saved above; a new agent's wait for its composer.
    let left = std::mem::take(&mut app.composer);
    app.drafted = None;
    super::start::put_away(app, left, old.is_some());
    completion::reset(app);
    super::split::start(app, old, now());
    recap::back(app);
    if let Some(id) = app.focus.clone() {
        app.open(id);
    }
}
