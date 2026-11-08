//! Where a new agent starts: every server's past folders and repositories, one row per
//! repository with the servers that have it. Choosing one aims the new-session composer at that
//! folder on that server; its model chip, `/model` and first message then work as at a launch.
use super::{App, Composer, fail, picker, text::*, view::Action, view::Hit};
use crate::{protocol::SessionId, terminal::bindings};
use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as Mods};
use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use reqwest::Method;
use serde::Deserialize;
use std::{cmp::Reverse, collections::BTreeMap, path::PathBuf};

/// A folder on a server; `at` is when a session last started there, none for a found checkout.
#[derive(Deserialize)]
struct Place {
    path: PathBuf,
    identity: String,
    at: Option<u64>,
}

#[derive(Default)]
pub struct Start {
    // Each reachable server by enrollment name (empty for this one): its label, and its places
    // once it answers.
    servers: BTreeMap<String, (String, Option<Vec<Place>>)>,
    filter: String,
    // The chosen project and server by name, so a late answer that moves the rows keeps them.
    chosen: Option<(String, String)>,
}

/// A project and its places, one a server.
type Project<'a> = (String, String, Vec<(&'a str, &'a Place)>);

impl Start {
    /// One row per repository, or folder outside git, matching the filter: recently used first,
    /// then found ones; each with its newest folder on each server, newest first, and named by
    /// that folder as the agents list names a project, not by its remote.
    fn projects(&self) -> Vec<Project<'_>> {
        let mut by = BTreeMap::<String, BTreeMap<&str, &Place>>::new();
        for (server, (_, places)) in &self.servers {
            for p in places.iter().flatten() {
                let newest = by
                    .entry(p.identity.clone())
                    .or_default()
                    .entry(server)
                    .or_insert(p);
                *newest = if p.at > newest.at { p } else { newest };
            }
        }
        let mut all: Vec<_> = by
            .into_iter()
            .map(|(identity, places)| {
                let mut places: Vec<_> = places.into_iter().collect();
                places.sort_by_key(|(_, p)| Reverse(p.at));
                let name = places[0].1.path.file_name().unwrap_or_default();
                (identity, name.to_string_lossy().into_owned(), places)
            })
            .collect();
        let mut names = BTreeMap::new();
        for (_, name, _) in &all {
            *names.entry(name.clone()).or_insert(0) += 1;
        }
        for (identity, name, _) in &mut all {
            if names[name] > 1 {
                *name = format!(
                    "{name} · {}",
                    identity
                        .split_once(':')
                        .map_or(identity.as_str(), |(_, id)| id)
                );
            }
        }
        let filter = self.filter.to_lowercase();
        all.retain(|(_, name, _)| name.to_lowercase().contains(&filter));
        all.sort_by_key(|(_, _, places)| Reverse(places[0].1.at));
        all
    }

    /// The chosen project's row and server's place: the first and its newest until chosen.
    fn at(&self, projects: &[Project]) -> (usize, usize) {
        let Some((name, server)) = &self.chosen else {
            return (0, 0);
        };
        let row = projects.iter().position(|p| &p.0 == name).unwrap_or(0);
        let places = projects.get(row).map_or(&[][..], |p| &p.2[..]);
        (row, places.iter().position(|p| p.0 == server).unwrap_or(0))
    }

    fn select(&mut self, row: usize, place: usize) {
        self.chosen = self
            .projects()
            .get(row)
            .and_then(|p| Some((p.0.clone(), p.2.get(place)?.0.into())));
    }

    /// The chosen server and folder.
    fn target(&self) -> Option<(String, PathBuf)> {
        let projects = self.projects();
        let (row, place) = self.at(&projects);
        let (server, p) = projects.get(row)?.2.get(place)?;
        Some((server.to_string(), p.path.clone()))
    }
}

/// Opens the chooser beside the agents list and asks every reachable server for its projects;
/// each answer lands on its own, so a slow server holds up none of the others.
pub fn open(app: &mut App) {
    if !app.list.open {
        super::agents::toggle(app);
    }
    let mut start = Start::default();
    for s in app.servers.iter().filter(|s| s.silent.is_none()) {
        start
            .servers
            .insert(s.name.clone(), (s.label.clone(), None));
    }
    for name in start.servers.keys() {
        let mut path = format!("v1/repos?server={name}");
        if name.is_empty()
            && let Ok(cwd) = std::env::current_dir()
        {
            let mut url = reqwest::Url::parse("http://localhost/").expect("valid static URL");
            url.query_pairs_mut()
                .append_pair("under", &cwd.to_string_lossy());
            path.push('&');
            path.push_str(url.query().unwrap_or_default());
        }
        let name = name.clone();
        app.spawn(|api| async move {
            let invalid = "A server's projects are invalid; reconnect to a matching server.";
            let read = api.json(Method::GET, &path, None).await;
            let places = read
                .and_then(|v| serde_json::from_value(v["repos"].clone()).map_err(fail(invalid)));
            move |app: &mut App| {
                if let Err(e) = &places {
                    app.say(&e.to_string());
                }
                let Some(start) = &mut app.start else { return };
                if start.chosen.is_none() {
                    start.select(0, 0);
                }
                if let Some(s) = start.servers.get_mut(&name) {
                    s.1 = Some(places.unwrap_or_default());
                }
            }
        });
    }
    (app.start, app.list.keys) = (Some(start), false);
}

/// Takes every key while open: typing filters, Up and Down choose the project, Left and Right
/// its server, Enter aims the composer there and Escape closes.
pub fn key(app: &mut App, k: KeyEvent) {
    let Some(start) = &mut app.start else { return };
    let projects = start.projects();
    let ((row, place), rows) = (start.at(&projects), projects.len().saturating_sub(1));
    let places = projects.get(row).map_or(0, |p| p.2.len().saturating_sub(1));
    match k.code {
        K::Esc => app.start = None,
        K::Enter => choose(app, None),
        K::Up => start.select(row.saturating_sub(1), 0),
        K::Down => start.select((row + 1).min(rows), 0),
        K::Left => start.select(row, place.saturating_sub(1)),
        K::Right => start.select(row, (place + 1).min(places)),
        K::Backspace | K::Char(_) if !k.modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
            match k.code {
                K::Char(c) => start.filter.push(c),
                _ => drop(start.filter.pop()),
            }
            start.chosen = None;
        }
        _ => {}
    }
}

/// Aims the new-session composer at the chosen folder on the chosen server, or at the clicked
/// one as it was drawn: the next first message starts it there.
pub fn choose(app: &mut App, clicked: Option<(String, PathBuf)>) {
    let chosen = app
        .start
        .as_ref()
        .and_then(|s| clicked.or_else(|| s.target()));
    if let Some((server, cwd)) = chosen {
        aim(app, server, cwd, None);
    }
}

/// Aims the new-session composer at `cwd`, or at a fresh thread of `resident` (its native name),
/// on `server`: the next first message starts it there.
pub fn aim(app: &mut App, server: String, cwd: PathBuf, resident: Option<SessionId>) {
    if app.waiting() {
        return app.say("A note, input or draft is pending; wait before starting a new agent.");
    }
    let to = (server, cwd, resident);
    if room(app, Some(&to)) {
        app.save(Some(Box::new(|app: &mut App| reaim(app, to))));
    }
}

// Aims once the server holds the addressed session's words.
fn reaim(app: &mut App, to: Destination) {
    super::recap::leave(app);
    for id in [app.root.take(), app.focus.clone()].into_iter().flatten() {
        app.streams.remove(&id).iter().for_each(|task| task.abort());
    }
    let left = std::mem::take(&mut app.composer);
    app.drafted = None;
    let from = app.focus.take();
    app.closing = None;
    app.conversations.clear();
    put_away(app, left, from.is_some());
    if from.is_some() {
        app.back = from;
    }
    let selected = (to.2.as_ref()).map_or(Action::New, |r| Action::Thread(r.on(&to.0)));
    // Only words typed for this same destination come back.
    app.composer = app.new_drafts.remove(&to).unwrap_or_default();
    (app.server, app.cwd, app.resident) = to;
    (app.start, app.initial, app.request) = (None, None, None);
    app.list.selected = Some(selected);
    super::composer::completion::reset(app);
    picker::request(app, true);
}

/// The new agents whose words may wait at once, in the app: a person types into a handful of new
/// destinations before sending, and each holds up to the editor's 1 MiB and its images.
pub const NEW_DRAFTS: usize = 8;

/// Where a new agent starts: server, folder and the resident it is a thread of.
pub type Destination = (String, PathBuf, Option<SessionId>);

// The new agent the editor is aimed at, while no session is addressed.
fn aimed(app: &App) -> Destination {
    (app.server.clone(), app.cwd.clone(), app.resident.clone())
}

/// Whether the editor's words may be left, for the new agent `to` if any: a session's are saved
/// first, while a new agent's must fit in `App.new_drafts`, where `to`'s make room; says why not.
pub fn room(app: &mut App, to: Option<&Destination>) -> bool {
    let words = &app.composer;
    let back = to.is_some_and(|to| app.new_drafts.contains_key(to));
    let refusal = match () {
        _ if app.focus.is_some() || words.is_empty() => return true,
        _ if words.text.len() > super::composer::DRAFT_BYTES => {
            "These words are past the 1 MiB cap; shorten them before leaving."
        }
        _ if app.new_drafts.len() >= NEW_DRAFTS && !back => {
            "Words for 8 new agents already wait; send, stash or clear these before leaving."
        }
        _ => return true,
    };
    app.say(refusal);
    false
}

/// Puts the words just left away. A session's are on the server already, without their images,
/// which drafts do not keep; a new agent's wait under the destination they were typed for.
pub fn put_away(app: &mut App, words: Composer, session: bool) {
    if session && words.active_images().next().is_some() {
        app.say("Drafts keep words only; the images in the words left were dropped.");
    } else if !session && !words.is_empty() {
        app.new_drafts.insert(aimed(app), words);
    }
}

/// While a quit is armed, what it would lose, wrapped to `columns`: no session or file holds a
/// new agent's words.
pub fn unsent(app: &App, columns: usize) -> Vec<Line<'static>> {
    let name = |(server, cwd, resident): &Destination| {
        let to = resident.as_ref().map_or_else(
            || format!("a new agent in {}", super::mcp::home(cwd)),
            |r| format!("a thread with {r}"),
        );
        match server.is_empty() {
            true => to,
            false => format!("{to} on {server}"),
        }
    };
    let to = aimed(app);
    let typed = (app.focus.is_none() && !app.composer.is_empty()).then_some(&to);
    let names = (app.new_drafts.keys().chain(typed).map(name)).collect::<Vec<_>>();
    if !app.quitting || names.is_empty() {
        return Vec::new();
    }
    let (names, back) = (names.join(", "), "/new or /thread there keeps them.");
    let lost = format!("Quitting loses the unsent words for {names}; {back}");
    inset(vec![span(lost, WHITE)], columns)
}

/// Draws the chooser over the foot of `area`, as the model picker draws, and records its clicks.
pub fn draw(app: &App, b: &mut Buffer, area: Rect, hits: &mut Vec<Hit>) {
    let Some(start) = &app.start else { return };
    let projects = start.projects();
    let rows = (area.height.saturating_sub(2) as usize).min(projects.len() + 2) as u16;
    let r = Rect::new(area.x, area.bottom().saturating_sub(rows), area.width, rows);
    let (x, w) = (r.x + 1, r.width.saturating_sub(2));
    fill(b, r, style(DATA, RECESS));
    let waiting = start.servers.values().any(|s| s.1.is_none());
    let note = ["", "asking the servers… · "][usize::from(waiting)];
    let keys = bindings::text(
        app,
        &format!(
            "new agent · {note}type to filter · <start.up>/<start.down> project · <start.left>/<start.right> server · <start.accept> · <start.cancel>"
        ),
    );
    put(b, x, r.y, &keys, w, style(GRAY, RECESS));
    let filter = format!("❯ {}", start.filter);
    put(b, x, r.y + 1, &filter, w, style(WHITE, RECESS));
    let (row, place) = start.at(&projects);
    let first = row.saturating_sub((rows as usize).saturating_sub(3));
    let shown = projects.iter().enumerate().skip(first);
    for ((i, (_, name, places)), y) in shown.zip(r.y + 2..r.bottom()) {
        let mine = i == row;
        let used = style([GRAY, WHITE][usize::from(places[0].1.at.is_some())], RECESS);
        let name = format!(
            "{} {:<24}",
            [" ", "›"][usize::from(mine)],
            clip(name, w.saturating_sub(14) as usize)
        );
        let ink = if mine { bold(CYAN, RECESS) } else { used };
        let mut end = put(b, x, y, &name, w, ink);
        let aim = |(server, p): &(&str, &Place)| Action::Choose(server.to_string(), p.path.clone());
        hits.push((Rect::new(r.x, y, r.width, 1), aim(&places[0])));
        for (at, (server, _)) in places.iter().enumerate() {
            let on = mine && at == place;
            let label = format!(" {} ", start.servers.get(*server).map_or("", |s| &s.0));
            let chip = [style(GRAY, RECESS), bold(GLASS, CYAN)][usize::from(on)];
            let next = put(b, end + 1, y, &label, (x + w).saturating_sub(end + 1), chip);
            let r = Rect::new(end + 1, y, next - end - 1, 1);
            hits.push((r, aim(&places[at])));
            end = next;
        }
    }
}
