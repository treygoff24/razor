//! Paging the record into the transcript: the page the reader reached, read beside the loop,
//! and the reader's place kept while it folds.
use super::{Direction, ITEMS, MOUNT, Transcript, item};
use crate::protocol::{EntryId, Event};
use crate::terminal::{App, Error, api, api::Page, error, fail};

impl Transcript {
    // The page the reader has reached the edge of, if one is wanted: older entries before the
    // first retained one, newer ones after the last, or the newest page again.
    pub(super) fn page_edge(&self) -> Option<(Direction, Option<EntryId>)> {
        if self.loading || self.opened.is_some() {
            return None;
        }
        let at = match self.wanted? {
            Direction::Older if self.first_row == 0 && self.mounted() >= self.entries.len() => {
                Some(self.first().filter(|id| id.0 > 1)?)
            }
            Direction::Newer if self.first_row + self.height >= self.total => Some(
                *self
                    .entries
                    .last_key_value()
                    .filter(|(id, _)| **id < self.cursor)?
                    .0,
            ),
            Direction::Newest | Direction::At(..) => None,
            _ => return None,
        };
        Some((self.wanted?, at))
    }

    // Before a page folds: the held view keeps the reader's place across it, even when admitting
    // an older page evicts this window's newest end.
    pub(in crate::terminal) fn prepare_page(&mut self, direction: Direction) {
        if matches!(direction, Direction::Newest | Direction::At(..)) {
            self.retain_older = false;
            while !self.entries.is_empty() {
                self.evict();
            }
            (self.held, self.wanted, self.mounted) = (None, None, MOUNT);
            return;
        }
        self.retain_older = direction == Direction::Older;
        self.mounted = (self.mounted.max(MOUNT) + 256).min(ITEMS);
        self.children_at = None;
        self.dirty = true;
    }

    /// After a page folds: a page of no items reads further back; at the newest end a window the
    /// reader follows, or did not page back, keeps its newest entries again, and a followed one
    /// stops reading back.
    pub(in crate::terminal) fn read_past_metadata(&mut self) {
        if !self.entries.values().any(item) {
            self.wanted = Some(Direction::Older);
        } else if (self.held.is_none() || self.wanted != Some(Direction::Older)) && self.newest() {
            (self.retain_older, self.wanted) = (false, self.wanted.filter(|_| self.held.is_some()));
        }
    }
}

/// Reads the history page the focused transcript's reader has reached, if any.
pub(in crate::terminal) fn read_visible(app: &mut App) {
    let Some(id) = app.focus.clone() else { return };
    let Some(t) = app.transcripts.get_mut(&id) else {
        return;
    };
    let Some((direction, at)) = t.page_edge() else {
        return;
    };
    if app.pending >= 8 {
        return;
    }
    t.loading = true;
    let held = t.held;
    app.spawn(|api| async move {
        let read = async {
            match direction {
                Direction::Older => api.page(&id, at.map(|id| id.0), 256).await,
                Direction::Newer => newer(&api, &id, at.unwrap_or_default()).await,
                Direction::Newest => api.page(&id, None, 256).await,
                Direction::At(at, _) => newer(&api, &id, EntryId(at.0.saturating_sub(1))).await,
            }
        };
        let timeout = "Reading transcript history timed out; try its page again.";
        let page = tokio::time::timeout(std::time::Duration::from_secs(30), read).await;
        let page = page.map_err(fail(timeout)).and_then(|page| page);
        move |app: &mut App| paged(app, id, direction, held, page)
    });
}

// Entry ids are record ordinals, so the before/limit route walks forward too: narrow the end
// whenever its byte bound skipped a large next entry.
async fn newer(
    api: &api::Api,
    id: &crate::protocol::SessionId,
    after: EntryId,
) -> Result<Page, Error> {
    let mut before = after.0.saturating_add(257);
    loop {
        let mut page = api.page(id, Some(before), 256).await?;
        let ids = page.events.iter().filter_map(|event| match event {
            Event::Entry(entry) => Some(entry.id.0),
            _ => None,
        });
        match ids.min().filter(|first| *first > after.0.saturating_add(1)) {
            Some(first) if first >= before => {
                return Err(error(
                    "The history page did not move; reconnect with a matching server.",
                ));
            }
            Some(first) => before = first,
            None => {
                page.events
                    .retain(|event| matches!(event, Event::Entry(entry) if entry.id > after));
                return Ok(page);
            }
        }
    }
}

fn paged(
    app: &mut App,
    id: crate::protocol::SessionId,
    direction: Direction,
    held: Option<(EntryId, isize)>,
    page: Result<Page, Error>,
) {
    let Some(t) = app.transcripts.get_mut(&id) else {
        return;
    };
    t.loading = false;
    if app.focus.as_ref() != Some(&id) || t.wanted != Some(direction) || t.held != held {
        return;
    }
    let page = match page {
        Ok(page) if !page.events.is_empty() => page,
        failed => {
            t.wanted = None;
            return failed.err().iter().for_each(|e| app.say(&e.to_string()));
        }
    };
    t.prepare_page(direction);
    for event in page.events {
        app.fold(Some(id.clone()), event, crate::protocol::now());
    }
    if let Some(t) = app.transcripts.get_mut(&id) {
        t.read_past_metadata();
        if let Direction::At(at, card) = direction {
            if card {
                t.open(at);
            }
            (t.wanted, t.held) = (Some(Direction::Older), Some((at, 0)));
        }
    }
}

/// Jump to an entry, optionally opening its card, using the same paging as history and search.
pub(in crate::terminal) fn jump(
    app: &mut App,
    session: &crate::protocol::SessionId,
    id: EntryId,
    card: bool,
) {
    let Some(t) = app.transcripts.get_mut(session) else {
        return;
    };
    match t.entries.contains_key(&id) {
        true => {
            let after = t.entries.range(id..).filter(|(_, e)| item(e)).count();
            t.mounted = t.mounted.max(after).min(ITEMS);
            if card {
                t.open(id);
            }
            (t.held, t.wanted, t.dirty) = (Some((id, 0)), Some(Direction::Older), true);
        }
        false => {
            t.opened = None;
            t.wanted = Some(Direction::At(id, card));
        }
    }
}
