//! A committed reply's optional next message; accepting only edits the ordinary draft.
use crate::protocol::{EntryId, State};
use crate::terminal::App;

impl App {
    /// An offer for the addressed, idle session, with no competing composer interaction.
    pub(in crate::terminal) fn suggestion(&self) -> Option<(EntryId, &str)> {
        let server = super::super::status::server(self, self.focus.as_ref()?)?;
        if !server.suggestions
            || !self.composer.text.is_empty()
            || self.composer.active_images().next().is_some()
            || self.loading_images > 0
            || self.sending.is_some()
            || super::super::audio::unsent(self)
            || self.picker.is_some()
            || self.panel.is_some()
            || self.start.is_some()
            || self.goto.is_some()
            || self.grid.is_some()
            || self.list.overview.is_some()
            || self.list.keys
            || self.selected.is_some()
            || self.answering.is_some()
        {
            return None;
        }
        let row = self.row()?;
        // Only this session's own open question hides its offer; another session's leaves it.
        if row.state != State::NeedsYou || row.settled.is_some() || !row.asks.is_empty() {
            return None;
        }
        // The server's summary publishes this id only after a successful, still-idle turn.
        let id = row.suggestion?;
        let t = self.transcript()?;
        if !t.queued.is_empty() || t.dismissed_suggestion == Some(id) {
            return None;
        }
        Some((id, t.entries.get(&id)?.suggestion.as_deref()?))
    }

    /// Remembers dismissal even while the draft or another interaction hides the candidate. Words
    /// for a question's answer are not this session's next message, so they dismiss nothing.
    pub(in crate::terminal) fn dismiss_suggestion(&mut self) {
        if self.answering.is_some() {
            return;
        }
        if let Some(id) = self.row().and_then(|r| r.suggestion)
            && let Some(t) = self.transcript_mut()
        {
            t.dismissed_suggestion = Some(id);
        }
    }

    pub(in crate::terminal) fn accept_suggestion(&mut self) {
        let Some((_, text)) = self.suggestion() else {
            return;
        };
        let text = text.to_owned();
        if self.composer.append(&text) {
            self.edited();
        }
    }
}
