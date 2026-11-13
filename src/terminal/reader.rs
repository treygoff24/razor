//! A delivery in the existing conversation card. Source arrives in bounded ranges; closing
//! the card drops its source; an already pending range finishes and is discarded.
use super::{
    App, api, bindings, clipboard,
    keys::Pane,
    markdown,
    panel::{self, Panel},
    text::*,
    view::Action,
};
use crate::protocol::{Body, Delivery, EntryId, Event as ApiEvent, SessionId};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
};
use reqwest::Method;
use std::sync::Arc;

// All 77 observed text deliveries fit 256 KiB. Release scroll at 80 columns:
// Markdown prose 12.5 ms, tables 23.3 ms; verbatim ordinary/single line 9.8/16.9 ms.
const SOURCE: usize = 256 << 10;

pub(super) struct Reader {
    delivery: Delivery,
    source: Vec<u8>,
    size: Option<u64>,
    error: Option<String>,
    visit: Arc<()>,
}

impl Reader {
    fn extension(&self) -> String {
        self.delivery
            .file
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
    }
    fn text(&self) -> Option<&str> {
        if self.delivery.url.is_some()
            || matches!(self.extension().as_str(), "html" | "htm" | "pdf")
        {
            return None;
        }
        match std::str::from_utf8(&self.source) {
            Ok(text) => Some(text),
            Err(e) if e.error_len().is_none() => {
                // A range or the cap can end inside a character; draw its complete prefix.
                std::str::from_utf8(&self.source[..e.valid_up_to()]).ok()
            }
            Err(_) => None,
        }
    }
}

/// Opens a delivery and marks it read through the same route as the other clients.
pub(super) fn open(app: &mut App, session: SessionId, entry: EntryId) {
    let Some(delivery) = app
        .delivered
        .iter()
        .find(|d| d.session == session && d.entry == entry)
        .cloned()
    else {
        return;
    };
    if delivery.voice.is_some() {
        app.panel = None;
        app.keys = Pane::Status;
        return super::audio::play(app, session, entry);
    }
    if app.focus.is_none() {
        return app.say("Address a session before opening a delivery.");
    }
    let read = delivery.read;
    app.status_selected = Some(Action::Delivery(
        session.clone(),
        entry,
        delivery.url.as_deref().map(Into::into),
    ));
    panel::open(
        app,
        Panel::Delivery(Reader {
            delivery: delivery.clone(),
            source: Vec::new(),
            size: None,
            error: None,
            visit: Arc::new(()),
        }),
    );
    if !read {
        app.write(|api| async move {
            let result = api
                .json(
                    Method::PUT,
                    &format!("v1/sessions/{session}/deliveries/{}/read", entry.0),
                    None,
                )
                .await;
            move |app: &mut App| result.err().iter().for_each(|e| app.say(&e.to_string()))
        });
    }
}

/// Starts one bounded range; each arrival is drawn before the next range completes.
pub(super) fn read_visible(app: &mut App) {
    let Some(Panel::Delivery(reader)) = &mut app.panel else {
        return;
    };
    let offset = reader.source.len() as u64;
    // A range under way holds the second reference to `visit`: that count is the only "loading" fact there is.
    if reader.text().is_none()
        || Arc::strong_count(&reader.visit) > 1
        || reader.error.is_some()
        || offset >= SOURCE as u64
        || reader.size.is_some_and(|size| offset >= size)
        || app.pending >= 8
    {
        return;
    }
    let delivery = reader.delivery.clone();
    let visit = reader.visit.clone();
    app.spawn(|api| async move {
        let result = async {
            if let Some(file) = delivery.file {
                return api
                    .chunk(
                        &delivery.session,
                        &file.to_string_lossy(),
                        offset,
                        api::CHUNK.min((SOURCE as u64) - offset),
                    )
                    .await;
            }
            let page = api
                .page(&delivery.session, Some(delivery.entry.0 + 1), 1)
                .await?;
            let text = page
                .events
                .into_iter()
                .find_map(|event| {
                    if let ApiEvent::Entry(e) = event
                        && e.id == delivery.entry
                        && let Body::Call { input, .. } = e.body
                    {
                        input["text"].as_str().map(str::to_owned)
                    } else {
                        None
                    }
                })
                .unwrap_or(delivery.line);
            let size = text.len() as u64;
            let mut bytes = text.into_bytes();
            bytes.truncate(SOURCE);
            Ok((bytes, size))
        }
        .await;
        move |app: &mut App| {
            if let Some(Panel::Delivery(reader)) = &mut app.panel
                && Arc::ptr_eq(&reader.visit, &visit)
            {
                match result {
                    Ok((bytes, size)) => {
                        reader.source.extend(bytes);
                        reader.size = Some(size);
                    }
                    Err(e) => reader.error = Some(e.to_string()),
                }
            }
        }
    });
}

/// Draws the existing card, retaining only the viewport's styled rows and links.
pub(super) fn draw(app: &mut App, b: &mut Buffer, r: Rect, edge: ratatui::style::Color) {
    let copy_hint = bindings::text(app, "<panel.cancel>, then <status.copy> in the inbox");
    let keys = bindings::hints(app, "<panel.cancel> back · <panel.page_down> page", " · ");
    let Some(Panel::Delivery(reader)) = &mut app.panel else {
        return;
    };
    let title = reader
        .delivery
        .file
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| reader.delivery.line.clone());
    let body = panel::frame(
        b,
        r,
        &clip(&screen_safe(&title), r.width.saturating_sub(8) as usize),
        edge,
    );
    let columns = body.width as usize;
    let text = reader.text();
    let hidden = reader
        .size
        .unwrap_or(0)
        .saturating_sub(text.map_or(0, str::len) as u64);
    let state = match &reader.error {
        Some(error) => error.clone(),
        None if text.is_some() && reader.source.len() == SOURCE && hidden > 0 => {
            format!(
                "{hidden} bytes not shown · {copy_hint} copies path; open whole file in phone/Mac app"
            )
        }
        None if text.is_some()
            && reader
                .size
                .is_none_or(|size| (reader.source.len() as u64) < size) =>
        {
            "Reading…".into()
        }
        _ => String::new(),
    };
    let warning = wrap(vec![Span::raw(screen_safe(&state).into_owned())], columns);
    let height = body.height.saturating_sub(1 + warning.len() as u16) as usize;

    let markdown =
        reader.delivery.file.is_none() || matches!(reader.extension().as_str(), "md" | "markdown");
    let hint = match (&reader.delivery.url, text) {
        (Some(url), _) => format!("{}\n\n{url}", reader.delivery.line),
        (_, None) => format!(
            "{}\n\nCannot draw this file here. Open it in the phone or Mac app; {copy_hint} copies its path.",
            reader.delivery.line
        ),
        _ => String::new(),
    };
    let source = text.unwrap_or(&hint);
    let first = app.panel_scroll;
    let range = first..first.saturating_add(height);
    let (rows, total) = if markdown && hint.is_empty() {
        let rendered = markdown::render(source, columns, false, None, range);
        (rendered.lines, rendered.total)
    } else {
        let mut total = 0;
        let mut rows = Vec::new();
        for text in source.lines() {
            visit_wrap(&[Span::raw(screen_safe(text))], columns, |parts, _, _| {
                if range.contains(&total) {
                    rows.push(markdown::Rendered {
                        words: Line::raw(parts.iter().map(|p| p.text).collect::<String>()),
                        ..Default::default()
                    });
                }
                total += 1;
                true
            });
        }
        (rows, total)
    };
    // One parse per frame: clamping a jump beyond the end asks the next frame to draw it.
    let clamped = first.min(total.saturating_sub(height));
    if first != clamped {
        app.panel_scroll = clamped;
        app.dirty = true;
    }
    if let Some(url) = &reader.delivery.url {
        let link = Rect::new(body.x, body.y, body.width, height as u16);
        app.links.push((link, url.as_str().into()));
    }
    for (y, row) in (body.y..).zip(rows) {
        line(b, body, y, &row.words, style(DATA, GLASS));
        for link in row.links {
            let x = body
                .x
                .saturating_add(link.columns.start.min(columns) as u16);
            let len = (link.columns.end - link.columns.start)
                .min(body.right().saturating_sub(x) as usize);
            if len > 0 {
                app.links.push((Rect::new(x, y, len as u16, 1), link.url));
            }
        }
    }
    for (y, words) in (body.y + height as u16..body.bottom().saturating_sub(1)).zip(warning) {
        line(b, body, y, &words, style(AMBER, GLASS));
    }
    put(
        b,
        body.x,
        body.bottom().saturating_sub(1),
        &keys,
        body.width,
        style(GRAY, GLASS),
    );
}

/// Copies a delivery’s path or link through the existing clipboard.
pub(super) fn copy(app: &mut App, session: &SessionId, entry: EntryId) {
    let id = (session, entry);
    if let Some(d) = app.delivered.iter().find(|d| (&d.session, d.entry) == id) {
        let path = d.path.as_ref().map(|p| p.display().to_string());
        let text = d.url.clone().or(path).unwrap_or_else(|| d.line.clone());
        clipboard::copy(app, Some(text));
    }
}
