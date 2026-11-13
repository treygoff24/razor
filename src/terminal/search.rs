//! The addressed transcript's last submitted search; no history beyond this result list.
use super::{
    App, Error,
    panel::{self, Panel},
    text::*,
    transcript,
};
use crate::protocol::{Found, SessionId};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Line;

const LIMIT: usize = 50;

#[derive(Default)]
pub(super) struct Search {
    query: String,
    session: Option<SessionId>,
    selected: usize,
    answer: Option<Result<Found, Error>>,
}

pub(super) fn open(app: &mut App, words: &str) {
    if app.focus.is_none() {
        return app.say("Open a session before searching its transcript.");
    }
    if app.search.as_ref().is_some_and(|s| s.session != app.focus) {
        app.search = None;
    }
    let selected = app.search.get_or_insert_default().selected;
    panel::open(app, Panel::Search);
    app.panel_scroll = selected;
    if !words.is_empty() {
        app.panel_search = Some(words.into());
        submit(app);
    } else if app.search.as_ref().is_some_and(|s| s.query.is_empty()) {
        app.panel_search = Some(String::new());
    }
}

fn submit(app: &mut App) {
    let Some(query) = app.panel_search.clone() else {
        return;
    };
    if query.trim().is_empty() {
        return;
    }
    let Some(s) = &mut app.search else { return };
    let Some(session) = app.focus.clone() else {
        return;
    };
    s.session = Some(session.clone());
    s.query = query.clone();
    s.answer = None;
    app.panel_scroll = 0;
    app.panel_search = None;
    let started = app.spawn(|api| async move {
        // A sequence of string pairs is always a valid form query.
        let q = serde_urlencoded::to_string([("q", &query)]).expect("string query pairs");
        let path = format!("/v1/sessions/{session}/search?{q}&limit={LIMIT}");
        let read = api.json(reqwest::Method::GET, &path, None).await;
        let answer = read.and_then(|v| {
            serde_json::from_value(v)
                .map_err(super::fail("The server sent an invalid search answer."))
        });
        move |app: &mut App| {
            if let Some(s) = &mut app.search
                && s.session.as_ref() == Some(&session)
                && s.query == query
            {
                s.answer = Some(answer);
            }
        }
    });
    if !started && let Some(s) = &mut app.search {
        s.answer = Some(Err(super::error("Search was not submitted; try again.")));
    }
}

/// Panel keys keep their usual navigation; Enter submits the search line or opens its row.
pub(super) fn key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc
            if app.panel_search.is_some()
                && app.search.as_ref().is_some_and(|s| s.query.is_empty()) =>
        {
            app.panel = None;
            app.panel_search = None;
        }
        KeyCode::Enter if app.panel_search.is_some() => submit(app),
        KeyCode::Enter => {
            if let Some(s) = &app.search
                && let Some(Ok(found)) = &s.answer
                && let Some(hit) = found.hits.get(app.panel_scroll)
                && let Some(session) = app.focus.clone()
                && s.session.as_ref() == Some(&session)
            {
                let entry = hit.entry;
                app.panel = None;
                transcript::jump(app, &session, entry, true);
            }
        }
        KeyCode::Char('/') if app.panel_search.is_none() => {
            app.panel_search = app.search.as_ref().map(|s| s.query.clone());
        }
        _ => return false,
    }
    true
}

pub(super) fn rows(app: &mut App, width: usize, height: usize) -> Vec<Line<'static>> {
    let Some(s) = &mut app.search else {
        return Vec::new();
    };
    let query = app.panel_search.as_deref().unwrap_or(&s.query);
    let edit = if app.panel_search.is_some() {
        "▏"
    } else {
        ""
    };
    let label = format!("/ {}{edit}", screen_safe(query));
    let said = match &s.answer {
        None if s.query.is_empty() => "Words in any order; each may be the start of a word.".into(),
        None => "Searching…".into(),
        Some(Err(e)) => e.to_string(),
        Some(Ok(f)) if f.hits.is_empty() => "No matches in this session.".into(),
        Some(Ok(f)) if f.hits.len() == LIMIT => format!("{LIMIT} rows · add a word to narrow."),
        Some(Ok(f)) => format!("{} found", f.hits.len()),
    };
    let hits = s
        .answer
        .as_ref()
        .and_then(|a| a.as_ref().ok())
        .map_or(&[][..], |f| &f.hits);
    app.panel_scroll = app.panel_scroll.min(hits.len().saturating_sub(1));
    s.selected = app.panel_scroll;
    let selected = s.selected;
    let mut rows = vec![Line::raw(label), Line::styled(said, style(GRAY, GLASS))];
    let narrow = width < super::view::PHONE as usize;
    let room = height.saturating_sub(2) / (1 + usize::from(narrow));
    let first = selected.saturating_sub(room.saturating_sub(1));
    for (i, hit) in hits.iter().enumerate().skip(first).take(room) {
        let title = format!(
            "{} · {}",
            clip(&screen_safe(super::tools::kind(&hit.from)), 12),
            clock(hit.at, app.zone.as_ref())
        );
        let mut row = super::goto::hit_row(hit, &title, 30.min(width.saturating_sub(4)));
        let snippet = narrow.then(|| row.spans.split_off(1));
        if i == selected {
            row.spans[0].content = format!("❯{}", &row.spans[0].content[1..]).into();
            row.style = style(CYAN, YOU_BG);
        }
        rows.push(row);
        rows.extend(snippet.map(Line::from));
    }
    rows
}
