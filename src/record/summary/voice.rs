//! The phone's live calls and meetings as the summary holds them: the call facts no turn has
//! answered, how many of them were left out, and the newest meetings with how far each got.

use super::Summary;
use crate::protocol::{CallEvent, CallReason, EntryId, Source};

/// A meeting recorded on the phone, from its start fact: the phone's id for it, the folder
/// meetrec made, the phone that recorded it, and how far it got.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Meeting {
    pub(crate) id: String,
    pub(crate) folder: String,
    pub(crate) device: Option<String>,
    pub(crate) state: MeetingState,
}

/// Recording from its start, stopped by the person's end, and closed by its notes input or a
/// cancel; it only moves forward. The batch pass's outcome is the folder's `final.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MeetingState {
    Recording,
    Stopped,
    Closed,
}

impl Summary {
    /// A live call's or a meeting's fact, unread until a turn answers it: past the newest 256
    /// lines the oldest line is left out, and past 512 facts the oldest fact.
    pub(super) fn voice_call(
        &mut self,
        id: EntryId,
        call: &str,
        source: Source,
        event: &CallEvent,
    ) {
        if source == Source::Meeting {
            self.meeting(call, event);
        }
        let facts = &mut self.voice_calls;
        facts.push((id, matches!(event, CallEvent::Line { .. })));
        let lines = facts.iter().filter(|(_, line)| *line).count();
        let oldest = facts
            .iter()
            .position(|(_, line)| *line)
            .filter(|_| lines > 256);
        if let Some(at) = oldest.or((facts.len() > 512).then_some(0)) {
            facts.remove(at);
            self.voice_calls_left_out += 1;
        }
    }

    /// A turn's end answers the call facts it read; once it answers any, none are left out.
    pub(super) fn calls_answered(&mut self, inputs: &[EntryId]) {
        let facts = self.voice_calls.len();
        self.voice_calls.retain(|(id, _)| !inputs.contains(id));
        if self.voice_calls.len() < facts {
            self.voice_calls_left_out = 0;
        }
    }

    /// A start lists the meeting as the newest, recording; an end stops it and a cancel closes it.
    fn meeting(&mut self, call: &str, event: &CallEvent) {
        let state = match event {
            CallEvent::Started { folder, device } => {
                self.meetings.retain(|m| m.id != call);
                self.meetings.push(Meeting {
                    id: call.into(),
                    folder: folder.clone().unwrap_or_default(),
                    device: device.clone(),
                    state: MeetingState::Recording,
                });
                self.meetings
                    .drain(..self.meetings.len().saturating_sub(16));
                return;
            }
            CallEvent::Ended {
                reason: CallReason::Canceled,
            } => MeetingState::Closed,
            CallEvent::Ended { .. } => MeetingState::Stopped,
            _ => return,
        };
        self.advance_meeting(call, state);
    }

    pub(super) fn advance_meeting(&mut self, id: &str, state: MeetingState) {
        if let Some(meeting) = self.meetings.iter_mut().find(|m| m.id == id) {
            meeting.state = meeting.state.max(state);
        }
    }
}
