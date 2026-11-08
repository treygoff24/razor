//! One open archive page per owning server; a slow owner never holds up another.
use super::super::App;
use super::{Action, Row, panel, sections};
use crate::protocol::views::{SectionKind, ViewRow};
use crate::protocol::{AgentView, SessionId};
use std::collections::BTreeMap;

#[derive(Default)]
pub(in crate::terminal) struct Earlier {
    pages: BTreeMap<String, Page>,
    view: Option<(String, u64)>,
    request: u64,
}

#[derive(Default)]
struct Page {
    rows: Vec<AgentView>,
    after: Option<SessionId>,
    next: Option<SessionId>,
    error: Option<String>,
    loading: Option<u64>,
}

impl Earlier {
    pub(in crate::terminal) fn row(&self, id: &SessionId) -> Option<&AgentView> {
        self.pages
            .values()
            .flat_map(|p| &p.rows)
            .find(|a| &a.session == id)
    }
}

pub(in crate::terminal) fn sync(app: &mut App) {
    let view = app
        .list
        .status
        .as_ref()
        .filter(|s| s.error.is_none())
        .and_then(|s| app.list.view.clone().map(|v| (v, s.version)))
        .filter(|_| app.list.toggled.contains("earlier"));
    if view != app.list.earlier.view {
        app.list.earlier = Earlier {
            view,
            request: app.list.earlier.request,
            ..Default::default()
        };
    }
    if app.list.earlier.view.is_some() {
        let owners: Vec<_> = app.servers.iter().map(|s| s.name.clone()).collect();
        app.list
            .earlier
            .pages
            .retain(|name, _| owners.contains(name));
        for owner in owners {
            if !app.list.earlier.pages.contains_key(&owner) {
                load(app, owner, None);
            }
        }
    }
    install(app);
}

fn load(app: &mut App, owner: String, after: Option<SessionId>) {
    let Some(view) = app.list.earlier.view.clone() else {
        return;
    };
    let name = owner.clone();
    let cursor = after.clone();
    let request = app.list.earlier.request.wrapping_add(1);
    if app.pending >= 8
        || !app.spawn(|api| async move {
            let read = api.earlier(&name, &view.0, cursor.as_ref()).await;
            move |app: &mut App| {
                let Some(page) = app.list.earlier.pages.get_mut(&name) else {
                    return;
                };
                if page.loading != Some(request) {
                    return;
                }
                page.loading = None;
                match read {
                    Ok(read) => (page.rows, page.next, page.error) = (read.agents, read.next, None),
                    Err(e) => page.error = Some(e.to_string()),
                }
                app.revision = app.revision.wrapping_add(1);
            }
        })
    {
        return;
    }
    app.list.earlier.request = request;
    let page = app.list.earlier.pages.entry(owner).or_default();
    (page.after, page.loading) = (after, Some(request));
}

fn install(app: &mut App) {
    let earlier = &app.list.earlier;
    let mut agents: Vec<_> = earlier
        .pages
        .values()
        .flat_map(|p| &p.rows)
        .filter(|a| !app.agents.contains_key(&a.session))
        .collect();
    if earlier.view.is_some()
        && let Some(selected) = app.focus.as_ref().and_then(|id| app.agent_row(id))
        && !app.agents.contains_key(&selected.session)
        && !agents.iter().any(|a| a.session == selected.session)
    {
        agents.push(selected);
    }
    agents.sort_by(|a, b| {
        b.session
            .as_str()
            .split('@')
            .next()
            .cmp(&a.session.as_str().split('@').next())
    });
    let rows: Vec<_> = agents
        .into_iter()
        .map(|a| ViewRow {
            session: a.session.clone(),
            depth: 0,
            folded: false,
        })
        .collect();
    if let Some(status) = &mut app.list.status {
        for layout in &mut status.layouts {
            for section in layout
                .sections
                .iter_mut()
                .filter(|s| s.kind == SectionKind::Earlier)
            {
                section.sessions = if app.list.view.as_ref() == Some(&layout.name) {
                    rows.clone()
                } else {
                    Vec::new()
                };
            }
        }
    }
}

pub(super) fn press(app: &mut App) {
    let pages = &app.list.earlier.pages;
    let retry = pages
        .values()
        .any(|p| p.error.is_some() && p.loading.is_none());
    let queries: Vec<_> = pages
        .iter()
        .filter(|(_, p)| p.loading.is_none())
        .filter_map(|(name, p)| {
            if retry {
                p.error.as_ref().map(|_| (name.clone(), p.after.clone()))
            } else {
                p.next.clone().map(|next| (name.clone(), Some(next)))
            }
        })
        .collect();
    for (name, after) in queries {
        load(app, name, after);
    }
}

pub(super) fn controls(app: &App, w: usize, tone: panel::Tone) -> Vec<Row> {
    use super::super::text::{AMBER, CYAN, GRAY, span};
    let pages = &app.list.earlier.pages;
    let mut out = Vec::new();
    for (owner, page) in pages {
        let owner = [owner.as_str(), "this server"][usize::from(owner.is_empty())];
        let notice = match &page.error {
            Some(error) => format!("last heard · {owner}: {error}"),
            None if page.loading.is_some() => format!("{owner}: loading"),
            None => continue,
        };
        out.push(Row(
            sections::framed(
                span(" ", GRAY),
                vec![span(format!(" {notice}"), AMBER)],
                w,
                tone,
                None,
            ),
            None,
            None,
        ));
    }
    let retry = pages
        .values()
        .any(|p| p.error.is_some() && p.loading.is_none());
    if retry
        || pages
            .values()
            .any(|p| p.next.is_some() && p.loading.is_none())
    {
        let mark = [" ", "❯"][usize::from(app.list.selected == Some(Action::Earlier))];
        let word = if retry { "Retry" } else { "More" };
        let words = vec![span(format!(" {mark} {word}"), CYAN)];
        out.push(Row(
            sections::framed(span(" ", GRAY), words, w, tone, None),
            Some(Action::Earlier),
            None,
        ));
    }
    out
}
