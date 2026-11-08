//! A card over the transcript: `/status`, `/tasks`, `/login` with the server's sign-in
//! instructions, or `/mcp`.
use super::{App, mcp, text::*, view};
use crate::protocol::Login;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
};
use std::collections::VecDeque;

pub enum Panel {
    Status,
    /// The addressed project's grouped task list and selected task card.
    Tasks,
    Help,
    Keys,
    // Sign-in instructions are transient API output, at most 64 KiB in 128 lines.
    Login {
        lines: VecDeque<String>,
        running: bool,
    },
    /// `/mcp`, which lays out and keys its own views inside the box.
    Mcp(mcp::Mcp),
}

/// Opens the panel `word` names (`/status`, `/help`, `/keys`, `/tasks` or `/login`); false for
/// another command.
pub fn command(app: &mut App, word: &str, args: &str) -> bool {
    match word {
        "/status" => open(app, Panel::Status),
        "/help" => open(app, Panel::Help),
        "/keys" => open(app, Panel::Keys),
        "/tasks" => {
            (app.tasks.detail, app.tasks.pending) = (None, None);
            open(app, Panel::Tasks);
            if !args.is_empty() {
                match args.parse() {
                    Ok(id) => super::tasks::detail(app, id),
                    Err(_) => app.say("Use /tasks <number> to open a task."),
                }
            }
        }
        "/login" => start(app),
        _ => return false,
    }
    true
}

/// Shows `panel` from its top, unsearched.
pub fn open(app: &mut App, panel: Panel) {
    (app.panel, app.panel_scroll, app.panel_search) = (Some(panel), 0, None);
}

fn searchable(app: &App) -> bool {
    matches!(app.panel, Some(Panel::Keys | Panel::Help))
}

/// Starts the server's sign-in; Escape cancels it by dropping its stream.
fn start(app: &mut App) {
    let (lines, running) = (VecDeque::new(), true);
    open(app, Panel::Login { lines, running });
    let (Some(api), Some(tx), Some(budget)) =
        (app.api.clone(), app.lines.clone(), app.budget.clone())
    else {
        return;
    };
    let task = tokio::spawn(async move { api.login(tx, budget).await });
    app.login = Some(task.abort_handle());
    app.spawn(|_| async move {
        let ended = task.await;
        // A cancelled sign-in leaves the handle of any later one alone.
        move |app: &mut App| match ended {
            Ok(Ok(())) => {
                app.login = None;
                if let Some(Panel::Login { running, .. }) = &mut app.panel {
                    *running = false;
                }
            }
            Ok(Err(e)) => {
                app.login = None;
                show(app, &e.to_string(), true);
            }
            Err(_) => {}
        }
    });
}

/// One sign-in line from the server.
pub fn login(app: &mut App, line: Login) {
    match line {
        Login::Show { text } => show(app, &text, false),
        Login::Done => show(app, "The server login completed.", true),
        Login::Failed { error } => show(app, &error, true),
    }
}

fn show(app: &mut App, text: &str, ended: bool) {
    let Some(Panel::Login { lines, running }) = &mut app.panel else {
        return;
    };
    *running = !ended;
    for line in screen_safe(text).lines() {
        lines.push_back(clip(line, 8192));
        while lines.len() > 128 || lines.iter().map(String::len).sum::<usize>() > 64 << 10 {
            lines.pop_front();
        }
    }
}

/// One key: an open `/mcp` takes it first, and what it leaves scrolls or closes the panel; while
/// a search is typed, a character without Ctrl is its word.
pub fn key(app: &mut App, key: KeyEvent) {
    if matches!(app.panel, Some(Panel::Tasks)) && super::tasks::key(app, key) {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if matches!(app.panel, Some(Panel::Mcp(_))) && !ctrl && mcp::key(app, key) {
        return;
    }
    let searching = app.panel_search.is_some();
    app.panel_scroll = match key.code {
        KeyCode::Char(c) if searching && !ctrl => {
            app.panel_search.get_or_insert_default().push(c);
            0
        }
        KeyCode::Backspace if searching => {
            app.panel_search.get_or_insert_default().pop();
            0
        }
        KeyCode::Char('/') if searchable(app) => {
            app.panel_search = Some(String::new());
            0
        }
        KeyCode::Esc if searching => {
            app.panel_search = None;
            0
        }
        KeyCode::Esc => {
            if let (Some(Panel::Login { .. }), Some(task)) = (&app.panel, app.login.take()) {
                task.abort();
            }
            app.panel = None;
            0
        }
        KeyCode::Up => app.panel_scroll.saturating_sub(1),
        KeyCode::PageUp => app.panel_scroll.saturating_sub(20),
        KeyCode::Char('u') => app.panel_scroll.saturating_sub(10),
        KeyCode::Down => app.panel_scroll.saturating_add(1),
        KeyCode::PageDown => app.panel_scroll.saturating_add(20),
        KeyCode::Char('d') => app.panel_scroll.saturating_add(10),
        KeyCode::Home => 0,
        KeyCode::End => usize::MAX,
        _ => app.panel_scroll,
    };
}

/// The card in a rule box over `r`, two columns in from each side and edge to edge below
/// `view::PHONE`, scrolled, with its keys on the last row inside.
pub fn draw(app: &mut App, b: &mut Buffer, r: Rect, now: u64, hits: &mut Vec<view::Hit>) {
    if r.width < 8 || r.height < 4 {
        return;
    }
    let inset = if app.columns < view::PHONE { 0 } else { 2 };
    let r = Rect::new(r.x + inset, r.y, r.width - 2 * inset, r.height);
    let width = r.width.saturating_sub(4) as usize;
    let found = |rows| found(rows, app.panel_search.as_deref(), width);
    let (title, rows) = match &app.panel {
        Some(Panel::Status) => ("status", super::status::rows(app, width, now)),
        Some(Panel::Tasks) => return super::tasks::draw(app, b, r, hits),
        Some(Panel::Help) => ("help", found(super::composer::completion::help(width))),
        Some(Panel::Keys) => ("keys", found(super::bindings::rows(app))),
        Some(Panel::Login { lines, running, .. }) => {
            let rows = lines
                .iter()
                .flat_map(|s| wrap(vec![Span::raw(s.clone())], width));
            let title = ["login", "login · waiting for sign-in"][usize::from(*running)];
            (title, rows.collect())
        }
        Some(Panel::Mcp(_)) => return mcp::draw(app, b, r, now, hits),
        None => return,
    };
    let words = frame(b, r, title, BEZEL_HI);
    let body = Rect {
        height: words.height.saturating_sub(1),
        ..words
    };
    scroll(app, b, body, &rows);
    let search = match (&app.panel_search, searchable(app)) {
        (Some(words), _) => format!("/{words}▏ · "),
        (None, true) => "<panel.search> search · ".into(),
        (None, false) => String::new(),
    };
    let keys = format!("{search}<panel.up>/<panel.down> scroll · <panel.cancel> back");
    let keys = super::bindings::hints(app, &keys, " · ");
    put(
        b,
        words.x,
        body.bottom(),
        &keys,
        words.width,
        style(GRAY, GLASS),
    );
}

// The rows holding every searched word in any case, each wrapped to `width`.
fn found(rows: Vec<Line<'static>>, search: Option<&str>, width: usize) -> Vec<Line<'static>> {
    let words = search.unwrap_or_default().to_lowercase();
    let holds = |row: &Line| {
        let row = row.to_string().to_lowercase();
        words.split_whitespace().all(|word| row.contains(word))
    };
    let rows = rows.into_iter().filter(holds);
    rows.flat_map(|row| wrap(row.spans, width)).collect()
}

/// Fills `r` and draws its rule box in `colour` with `title` in the top edge; returns the area
/// inside for the words, two columns in from each side.
pub fn frame(b: &mut Buffer, r: Rect, title: &str, colour: ratatui::style::Color) -> Rect {
    fill(b, r, style(DATA, GLASS));
    let (edge, rule) = (style(colour, GLASS), "─".repeat(r.width as usize - 2));
    put(b, r.x, r.y, &format!("┌{rule}┐"), r.width, edge);
    let title = format!(" {title} ");
    put(b, r.x + 2, r.y, &title, r.width - 4, style(WHITE, GLASS));
    for y in r.y + 1..r.bottom() - 1 {
        put(b, r.x, y, "│", 1, edge);
        put(b, r.right() - 1, y, "│", 1, edge);
    }
    put(b, r.x, r.bottom() - 1, &format!("└{rule}┘"), r.width, edge);
    Rect::new(r.x + 2, r.y + 1, r.width - 4, r.height - 2)
}

/// Draws `rows` into `r` from the panel's scroll, first clamped so the last row reaches the
/// bottom and no further.
pub fn scroll(app: &mut App, b: &mut Buffer, r: Rect, rows: &[Line<'_>]) {
    let last = rows.len().saturating_sub(r.height as usize);
    app.panel_scroll = app.panel_scroll.min(last);
    for (words, y) in rows.iter().skip(app.panel_scroll).zip(r.y..r.bottom()) {
        line(b, r, y, words, style(DATA, GLASS));
    }
}
