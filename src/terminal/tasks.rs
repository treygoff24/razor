//! The addressed session's task list, shared by the status pane and task panel.
use super::{
    App, Error, markdown,
    panel::{self, Panel},
    text::*,
    view::{Action, Hit},
};
use crate::protocol::SessionId;
use crossterm::event::{KeyCode, KeyEvent};
use jiff::civil::{Date, DateTime};
use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use reqwest::Method;
use serde_json::Value;

type Row = (Line<'static>, Option<Action>);

#[derive(Default)]
pub(super) struct Tasks {
    session: Option<SessionId>,
    answer: Option<Answer>,
    reading: bool,
    reread: bool,
    generation: u64,
    pub(super) offset: Option<usize>,
    pub(super) detail: Option<Value>,
    list_scroll: usize,
    pub(super) pending: Option<u64>,
}

#[derive(Default)]
struct Answer {
    project: String,
    open: Vec<Value>,
    closed: Vec<Value>,
    counts: Value,
    error: Option<String>,
    more: u64,
}

pub(super) fn changed(app: &mut App, project: &std::path::Path, server: Option<&str>) {
    let answer = app.tasks.answer.as_ref();
    let matches =
        answer.is_none_or(|a| a.project.is_empty() || project == std::path::Path::new(&a.project));
    if let Some(id) = app.focus.clone()
        && id.as_str().split_once('@').map(|(_, s)| s) == server
        && matches
    {
        read(app, &id);
    }
}

pub(super) fn read(app: &mut App, session: &SessionId) {
    let t = &mut app.tasks;
    if t.session.as_ref() != Some(session) {
        *t = Tasks {
            session: Some(session.clone()),
            generation: t.generation,
            ..Tasks::default()
        };
    }
    if t.reading {
        t.reread = true;
        return;
    }
    t.generation = t.generation.wrapping_add(1);
    let generation = t.generation;
    let id = session.clone();
    app.tasks.reading = app.spawn(move |api| async move {
        let path = format!("v1/sessions/{id}/tasks");
        let open_path = format!("{path}?filter=open&limit=200");
        let closed_path = format!("{path}?filter=closed&mine=true&limit=200");
        let result = tokio::try_join!(
            api.json(Method::GET, &open_path, None),
            api.json(Method::GET, &closed_path, None)
        );
        move |app: &mut App| {
            let t = &mut app.tasks;
            if app.focus.as_ref() != Some(&id) || t.generation != generation {
                return;
            }
            t.reading = false;
            let mut answer = match result {
                Ok((open, closed)) if open["project"].is_string() => Answer {
                    project: field(&open, "project").to_owned(),
                    open: open["tasks"].as_array().cloned().unwrap_or_default(),
                    closed: closed["tasks"].as_array().cloned().unwrap_or_default(),
                    counts: open["counts"].clone(),
                    more: open["more"].as_u64().unwrap_or_default(),
                    error: None,
                },
                Ok(_) | Err(Error::Refused { status: 404, .. }) => Answer::default(),
                Err(e) => {
                    let mut answer = t.answer.take().unwrap_or_default();
                    answer.error = Some(e.to_string());
                    answer
                }
            };
            let today = jiff::Zoned::now().date().to_string();
            answer.open.sort_by_key(|task| {
                let held = task["held"].as_bool().unwrap_or(false);
                let due = field(task, "due");
                (!held, due.is_empty() || due >= today.as_str())
            });
            t.answer = Some(answer);
            if std::mem::take(&mut t.reread) {
                read(app, &id);
            } else if let Some(number) = t.pending.or(t.detail.as_ref().map(self::id)) {
                detail(app, number);
            }
        }
    });
}

fn id(task: &Value) -> u64 {
    task["id"].as_u64().unwrap_or_default()
}
fn field<'a>(task: &'a Value, name: &str) -> &'a str {
    task[name].as_str().unwrap_or_default()
}
fn title(t: &Tasks) -> String {
    let project = t.answer.as_ref().map_or("", |a| project(&a.project));
    let answer = t.answer.as_ref();
    let count = answer.map_or(0, |a| a.open.len() as u64 + a.more);
    format!("{project} · {count} open")
}
fn project(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or_default()
}

fn date(value: &str) -> String {
    if let Ok(at) = DateTime::strptime("%Y%m%d-%H%M%S", value) {
        at.strftime("%b %-d %H:%M").to_string()
    } else {
        Date::strptime("%F", value.get(..10).unwrap_or_default())
            .ok()
            .map_or(String::new(), |d| d.strftime("%b %-d").to_string())
    }
}

fn claim<'a>(app: &App, task: &'a Value) -> &'a str {
    let words = field(task, "claim");
    let focus = app.focus.as_ref();
    let owner = focus.and_then(|id| super::status::server(app, id));
    match (words.rsplit_once(" on "), owner) {
        (Some((name, host)), Some(s)) if host == s.machine => name,
        _ => words,
    }
}

fn row(app: &App, task: &Value, width: usize, today: Date) -> Row {
    let state = field(task, "state");
    let (mark, ink) = match state {
        "in progress" => ("●", GREEN),
        "closed" => ("✓", GREEN),
        "blocked" => ("◌", GRAY),
        "open" => ("◫", GRAY),
        _ => ("○", WHITE),
    };
    let due = field(task, "due").parse::<Date>().ok();
    let note = if state == "closed" {
        date(field(task, "closed"))
    } else if due == Some(today) {
        "due today".into()
    } else if let Some(due) = due {
        let word = if due < today { "overdue" } else { "due" };
        format!("{word} {}", date(&due.to_string()))
    } else if state == "in progress" {
        claim(app, task).to_owned()
    } else if let Some(n) = task["blocked_by"][0].as_u64() {
        format!("blocked by {n}")
    } else if let Some(p @ 0..=1) = task["priority"].as_u64() {
        format!("P{p}")
    } else {
        String::new()
    };
    let prefix = format!(" {mark} {}  ", id(task));
    let room = width.saturating_sub(super::text::width(&prefix));
    let note = clip(&screen_safe(&note), room.saturating_sub(13));
    let room = room.saturating_sub(super::text::width(&note));
    let title = clip(
        &screen_safe(field(task, "title")),
        room.saturating_sub(usize::from(!note.is_empty())),
    );
    let gap = " ".repeat(room.saturating_sub(super::text::width(&title)));
    let note_ink = if state != "closed"
        && due.is_some_and(|d| d >= today && d <= today.saturating_add(jiff::Span::new().days(7)))
    {
        DATA
    } else {
        GRAY
    };
    let words = vec![
        span(prefix, ink),
        span(title, DATA),
        span(gap + &note, note_ink),
    ];
    (Line::from(words), Some(Action::Task(id(task))))
}

fn message(t: &Tasks) -> Option<String> {
    match &t.answer {
        None if t.reading => Some("reading…".into()),
        None => Some("couldn't read tasks; open the session again".into()),
        Some(a) if a.error.is_none() && a.project.is_empty() => {
            Some("this server has no tasks view".into())
        }
        Some(a) => a
            .error
            .as_ref()
            .map(|e| format!("couldn't read tasks: {e}")),
    }
}

pub(super) fn pane(app: &App, width: usize) -> Vec<Row> {
    let t = &app.tasks;
    if t.session.as_ref() != app.focus.as_ref()
        || t.answer
            .as_ref()
            .is_some_and(|a| a.error.is_none() && a.open.is_empty() && a.closed.is_empty())
    {
        return Vec::new();
    }
    let heading = format!("TASKS · {}", title(t));
    let mut rows = vec![
        (Line::default(), None),
        (rule(&heading, Some("/tasks"), width), None),
    ];
    if let Some(words) = message(t) {
        rows.push((span(words, GRAY).into(), None));
        return rows;
    }
    let Some(a) = &t.answer else {
        return rows;
    };
    let today = jiff::Zoned::now().date();
    let count = a.open.len() + t.offset.map_or(0, |_| a.closed.len());
    let (offset, shown, previous, more) = super::status::page(t.offset, count, 3);
    if let Some((words, to)) = previous {
        rows.push((span(words, GRAY).into(), Some(Action::Tasks(to))));
    }
    let items = a.open.iter().chain(&a.closed);
    rows.extend(
        items
            .skip(offset)
            .take(shown)
            .map(|t| row(app, t, width, today)),
    );
    if t.offset.is_none() && (more + a.closed.len() > 0 || a.more > 0)
        || t.offset.is_some() && more > 0
    {
        let words = match t.offset {
            None if a.open.is_empty() => {
                format!("none open · ✓ {} done this session", a.closed.len())
            }
            None => format!(
                "▸ {} more · ✓ {} done this session",
                more as u64 + a.more,
                a.closed.len()
            ),
            Some(_) => format!("+{more} more"),
        };
        rows.push((
            span(words, GRAY).into(),
            Some(Action::Tasks(Some(t.offset.map_or(0, |_| offset + shown)))),
        ));
    }
    if t.offset.is_some() && more == 0 && a.more > 0 {
        rows.push((
            span(format!("{} more not listed", a.more), GRAY).into(),
            None,
        ));
    }
    rows
}

pub(super) fn detail(app: &mut App, number: u64) {
    let Some(id) = app.focus.clone() else {
        return;
    };
    if !matches!(app.panel, Some(Panel::Tasks)) {
        panel::open(app, Panel::Tasks);
    }
    if app.tasks.detail.is_none() {
        app.tasks.list_scroll = app.panel_scroll;
    }
    app.tasks.pending = Some(number);
    app.spawn(move |api| async move {
        let path = format!("v1/sessions/{id}/tasks/{number}");
        let result = api.json(Method::GET, &path, None).await;
        move |app: &mut App| {
            let t = &mut app.tasks;
            if app.focus.as_ref() == Some(&id)
                && t.pending == Some(number)
                && matches!(app.panel, Some(Panel::Tasks))
            {
                t.pending = None;
                match result {
                    Ok(task) => {
                        if t.detail.as_ref().map(self::id) != Some(number) {
                            app.panel_scroll = 0;
                        }
                        t.detail = Some(task);
                    }
                    Err(e) => app.say(&e.to_string()),
                }
            }
        }
    });
}

fn rows(app: &App, width: usize) -> Vec<Row> {
    let t = &app.tasks;
    let today = jiff::Zoned::now().date();
    if let Some(task) = &t.detail {
        return card(app, task, width, today);
    }
    let mut rows = Vec::new();
    if let Some(words) = message(t) {
        return vec![(span(words, GRAY).into(), None)];
    }
    let Some(a) = &t.answer else {
        return rows;
    };
    let name = app.row().map_or("", |r| r.name.as_str());
    let held = format!("HELD BY {}", name.to_uppercase());
    for (label, state) in [
        (held.as_str(), "held"),
        ("READY", "ready"),
        ("IN PROGRESS", "in progress"),
        ("BLOCKED", "blocked"),
        ("WITH OPEN PARTS", "open"),
        ("DONE THIS SESSION", "closed"),
    ] {
        let all = a.open.iter().chain(&a.closed);
        let items: Vec<_> = all
            .filter(|item| {
                if item["held"] == true {
                    state == "held"
                } else {
                    field(item, "state") == state
                }
            })
            .collect();
        let mut count = a.counts[state].as_u64().unwrap_or(items.len() as u64);
        if state == "in progress" {
            count =
                count.saturating_sub(a.open.iter().filter(|t| t["held"] == true).count() as u64);
        }
        if !items.is_empty() {
            rows.push((rule(&format!("{label} · {count}"), None, width), None));
        }
        rows.extend(items.into_iter().map(|t| row(app, t, width, today)));
    }
    rows
}

fn card(app: &App, task: &Value, width: usize, today: Date) -> Vec<Row> {
    let title = span(screen_safe(field(task, "title")).into_owned(), WHITE);
    let lines = wrap(vec![title], width);
    let mut rows: Vec<_> = lines.into_iter().map(|line| (line, None)).collect();
    let mut facts = String::new();
    if let Some(p @ 0..=1) = task["priority"].as_u64() {
        facts.push_str(&format!("P{p} · "));
    }
    if let Some(due) = task["due"].as_str().filter(|s| !s.is_empty()) {
        facts.push_str(&format!("due {} · ", date(due)));
    }
    facts.push_str(&format!("recorded by {}", field(task, "by")));
    rows.push((Line::raw(screen_safe(&facts).into_owned()), None));
    if let Some(parent) = task.get("parent").filter(|p| p.is_object()) {
        rows.push(row(app, parent, width, today));
    }
    let state = field(task, "state");
    let words = if state == "closed" {
        let closed = field(task, "closed");
        let reason = closed.split_once(' ').map_or("", |(_, reason)| reason);
        vec![
            span("✓", GREEN),
            span(format!(" closed {} · {reason}", date(closed)), GRAY),
        ]
    } else {
        vec![span(format!("{state} · {}", claim(app, task)), GRAY)]
    };
    rows.push((Line::from(words), None));
    for (label, key) in [("BLOCKED BY", "blockers"), ("PARTS", "parts")] {
        if let Some(items) = task[key].as_array().filter(|a| !a.is_empty()) {
            let open = items.iter().filter(|t| t["state"] != "closed");
            let open = open.count();
            let label = if key == "parts" {
                format!("PARTS · {open} of {} open", items.len())
            } else {
                label.into()
            };
            rows.push((rule(&label, None, width), None));
            for item in items {
                rows.push(row(app, item, width, today));
            }
        }
    }
    let mut prose = |heading: Line<'static>, text: &str| {
        rows.push((heading, None));
        rows.extend(
            markdown::render(text, width, false, None, 0..4096)
                .lines
                .into_iter()
                .map(|r| (r.words, None)),
        );
    };
    prose(rule("DESCRIPTION", None, width), field(task, "text"));
    if let Some(comments) = task["comments"].as_array().filter(|a| !a.is_empty()) {
        let label = format!("COMMENTS · {}", comments.len());
        prose(rule(&label, None, width), "");
        if let Some(more) = task["more"].as_u64().filter(|n| *n > 0) {
            prose(Line::raw(format!("{more} earlier comments not shown")), "");
        }
        for comment in comments {
            let by = field(comment, "by");
            let at = date(field(comment, "at"));
            let heading = Line::raw(format!("{by} · {at}"));
            prose(heading, field(comment, "text"));
        }
    }
    rows
}

pub(super) fn key(app: &mut App, key: KeyEvent) -> bool {
    if key.code == KeyCode::Esc
        && (app.tasks.detail.take().is_some() || app.tasks.pending.is_some())
    {
        app.tasks.pending = None;
        app.panel_scroll = app.tasks.list_scroll;
        return true;
    }
    false
}

pub(super) fn draw(
    app: &mut App,
    b: &mut Buffer,
    r: Rect,
    edge: ratatui::style::Color,
    hits: &mut Vec<Hit>,
) {
    let width = r.width.saturating_sub(4) as usize;
    let title = match &app.tasks.detail {
        Some(task) => format!("task {} · {}", id(task), project(field(task, "project"))),
        None => format!("tasks · {}", title(&app.tasks)),
    };
    let area = panel::frame(b, r, &title, edge);
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let (lines, ids): (Vec<_>, Vec<_>) = rows(app, width).into_iter().unzip();
    panel::scroll(app, b, body, &lines);
    for (y, action) in (body.y..body.bottom()).zip(ids.into_iter().skip(app.panel_scroll)) {
        if let Some(action) = action {
            hits.push((Rect::new(body.x, y, body.width, 1), action));
        }
    }
    put(
        b,
        area.x,
        body.bottom(),
        "/tasks <number> opens · Esc back",
        area.width,
        style(GRAY, GLASS),
    );
}
