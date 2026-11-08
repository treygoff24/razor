//! The streams, preview reads and cached rows of launched children's cards.
use super::{App, Error, api::Page, text::*, tools::child, transcript::Row};
use crate::protocol::{AgentDetail, AgentView, Body, SessionId, State};
use ratatui::text::Span;

/// The opened root and addressed conversation each retain one row and their direct children.
#[derive(Default)]
pub(super) struct Conversation {
    pub detail: Option<AgentDetail>,
    pub dirty: bool,
    pub pending: bool,
}

/// Applies one coalesced detail read, leaving changes during the read dirty.
pub(super) fn received(app: &mut App, id: &SessionId, mut read: Result<AgentDetail, Error>) {
    if let Ok(detail) = &mut read {
        for child in &mut detail.children {
            if child.model.is_empty()
                && let Some(known) = app.agent_row(&child.session)
            {
                child.model.clone_from(&known.model);
                child.effort = known.effort;
            }
        }
    }
    let Some(conversation) = app.conversations.get_mut(id) else {
        return;
    };
    conversation.pending = false;
    match read {
        Ok(detail) => {
            conversation.detail = Some(detail);
            super::commands::ended(app);
        }
        Err(e) => app.say(&e.to_string()),
    }
    app.revision = app.revision.wrapping_add(1);
}

/// The most cards whose children stream at once.
const STREAMS: usize = 8;
/// What a card keeps of a child it does not show in full, through the fold's own eviction: the
/// newest 32 entries, which is each page it reads, and the last 16 KiB of the reply being written.
pub const KEPT: (usize, usize) = (32, 16 << 10);

/// Retains shown conversations and their child pages, coalescing dirty detail reads.
fn read_conversations(app: &mut App) {
    let (root, focus) = (app.root.clone(), app.focus.clone());
    let shown = |id: &SessionId| root.as_ref() == Some(id) || focus.as_ref() == Some(id);
    app.conversations.retain(|id, _| shown(id));
    let kept = |id: &SessionId| {
        app.agents.contains_key(id)
            || app.conversations.values().any(|c| {
                c.detail
                    .as_ref()
                    .is_some_and(|d| d.children.iter().any(|a| &a.session == id))
            })
    };
    app.previews.retain(|id, _| kept(id));
    app.transcripts.retain(|id, _| shown(id) || kept(id));
    let reads: Vec<_> = app
        .conversations
        .iter()
        .filter(|(_, c)| c.dirty && !c.pending)
        .take(4usize.saturating_sub(app.pending))
        .map(|(id, _)| id.clone())
        .collect();
    for id in reads {
        let pending = id.clone();
        if app.spawn(|api| async move {
            let read = api.agent(&id).await;
            move |app: &mut App| received(app, &id, read)
        }) && let Some(c) = app.conversations.get_mut(&pending)
        {
            (c.dirty, c.pending) = (false, true);
        }
    }
}

/// Keeps the drawn cards' children current, after each draw. The first `STREAMS` working
/// children among the frame's cards, in draw order, stream on from their newest page; a stream
/// is dropped once its child stops working or its card leaves the frame. Every other card reads
/// its newest page after its activity moved: at most four reads in flight, transient failures
/// retried five seconds on and said once; a 4xx never retries. Addressed sessions keep streams.
pub fn read_visible(app: &mut App, now: u64) {
    read_conversations(app);
    let (root, focus) = (app.root.clone(), app.focus.clone());
    let shown = |id: &SessionId| root.as_ref() == Some(id) || focus.as_ref() == Some(id);
    let working = |id: &SessionId| {
        app.agent_row(id).is_some_and(|a| a.state == State::Working)
            && super::status::server(app, id).is_none_or(|s| s.silent.is_none())
    };
    let cards: Vec<_> = (app.targets.iter())
        .filter_map(|a| match a {
            super::view::Action::Child(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let wanted = cards.iter().filter(|id| !shown(id) && working(id));
    let wanted: Vec<_> = wanted.take(STREAMS).cloned().collect();
    app.streams.retain(|id, task| {
        let kept = shown(id) || wanted.contains(id);
        if !kept {
            task.abort();
        }
        kept
    });
    for id in cards {
        if super::status::server(app, &id).is_some_and(|s| s.silent.is_some()) {
            continue;
        }
        let Some(activity) = app.agent_row(&id).map(|a| a.last_activity) else {
            continue;
        };
        if shown(&id) || app.streams.contains_key(&id) {
            continue;
        }
        let read = app.previews.get(&id).copied();
        if matches!(read, Some((_, u64::MAX, false, true))) {
            continue;
        }
        if read == Some((activity, u64::MAX, false, false)) {
            if wanted.contains(&id) {
                let after = app.transcripts.get(&id).map_or(0, |t| t.cursor.0);
                app.stream(id, after);
            }
            continue;
        }
        if let Some((at, after, busy, _)) = read
            && (busy || at == activity && after > now)
        {
            // A failed read is retried when its wait ends.
            if !busy {
                app.next.at(after);
            }
            continue;
        }
        if app.pending >= 4 {
            continue;
        }
        app.previews.insert(
            id.clone(),
            (activity, now + 1000, true, read.is_some_and(|r| r.3)),
        );
        let model = read.is_none() && app.agent_row(&id).is_some_and(|a| a.model.is_empty());
        app.spawn(|api| async move {
            let (page, owner) = tokio::join!(api.page(&id, None, KEPT.0), async {
                if model {
                    api.agent(&id).await.ok()
                } else {
                    None
                }
            });
            move |app: &mut App| previewed(app, id, activity, page, owner)
        });
    }
}

// Folds a card's page; the next draw streams its child on from it if it is wanted.
fn previewed(
    app: &mut App,
    id: SessionId,
    activity: u64,
    page: Result<Page, Error>,
    owner: Option<AgentDetail>,
) {
    if !app.previews.contains_key(&id) {
        return;
    }
    if let Some(owner) = owner {
        for detail in app
            .conversations
            .values_mut()
            .filter_map(|c| c.detail.as_mut())
        {
            if let Some(child) = detail.children.iter_mut().find(|a| a.session == id) {
                child.model.clone_from(&owner.row.model);
                child.effort = owner.row.effort;
            }
        }
        app.revision = app.revision.wrapping_add(1);
    }
    let now = crate::protocol::now();
    match page {
        Ok(page) => {
            app.previews
                .insert(id.clone(), (activity, u64::MAX, false, false));
            page.events
                .into_iter()
                .for_each(|event| app.fold(Some(id.clone()), event, now));
        }
        Err(e) => {
            let said = app.previews.get(&id).is_some_and(|r| r.3);
            let retry = if e.refused() { u64::MAX } else { now + 5000 };
            app.previews.insert(id, (activity, retry, false, true));
            if !said {
                app.say(&e.to_string());
            }
        }
    }
}

/// Rebuilds cached cards at their next change or on new data. A look change repaints only the
/// viewport; a changed row count invalidates the transcript's counts.
pub fn prepare(app: &mut App, parent: &SessionId, columns: u16, now: u64) {
    let Some(t) = app.transcripts.get(parent) else {
        return;
    };
    if t.children_at.is_some_and(|(revision, width, next)| {
        revision == app.revision && width == columns && now < next
    }) {
        return;
    }
    let mut lights = std::mem::take(&mut app.lights);
    let launched = |a: &&AgentView| a.parent.as_ref() == Some(parent);
    let children: std::collections::BTreeMap<_, _> = app
        .agent_children(parent)
        .filter_map(|a| Some((a.launched?, a)))
        .collect();
    let mut cards = std::collections::BTreeMap::new();
    for entry in t
        .entries
        .values()
        .skip(t.entries.len().saturating_sub(t.mounted()))
    {
        let Some(child) = children
            .get(&entry.id)
            .filter(|_| matches!(entry.body, Body::Call { .. }))
        else {
            continue;
        };
        let (rows, next) = child::rows(app, child, entry, columns as usize, now, &mut lights);
        cards.insert(
            entry.id,
            (rows, next, (child.name.clone(), model(&child.model))),
        );
    }
    let selected = app
        .selected_child()
        .and_then(|id| app.agent_row(id))
        .filter(launched);
    let row = selected.and_then(|a| cards.get_mut(&a.launched?)?.0.get_mut(1));
    if let Some(row) = row {
        mark(row);
    }
    app.lights = lights;
    if let Some(t) = app.transcripts.get_mut(parent) {
        let size = t.children.len() != cards.len()
            || cards
                .iter()
                .any(|(id, (rows, ..))| t.children.get(id).is_none_or(|c| c.0.len() != rows.len()));
        let next = cards.values().fold(u64::MAX, |at, (_, next, _)| {
            at.min(next.look).min(next.size)
        });
        (t.children, t.children_at) = (cards, Some((app.revision, columns, next)));
        t.repaint(size);
    }
}

fn mark(row: &mut Row) {
    let mark = Span::styled("›", bold(CYAN, GLASS));
    if let Some(first) = row.words.spans.first().cloned() {
        if let Some(tail) = first.content.strip_prefix("│ ") {
            row.words.spans.splice(
                0..1,
                [
                    Span::styled("│", first.style),
                    mark,
                    Span::styled(tail.to_owned(), first.style),
                ],
            );
        } else if first.content == "│"
            && let Some(second) = row.words.spans.get_mut(1)
        {
            *second = mark;
        }
    }
}
