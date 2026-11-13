//! Idle handover and close, owned by the session task and its ordinary tick.

use super::*;

impl Session {
    /// Checks child grace, configured idle closes and the existing idle handover boundary.
    pub(in crate::controller) async fn tick(&mut self, stuck_after: Duration) -> Result<(), Error> {
        self.settings.read();
        self.stuck(stuck_after);
        self.record.mark(self.due()).await?;
        if self.state == State::Working || self.blocked {
            return Ok(());
        }
        if let Some(input) = self
            .turn
            .tick(&mut self.settings, self.record.summary().child_task)
        {
            self.admit(input).await?;
        }
        if self.state == State::Working || self.record.summary().closed {
            return Ok(());
        }
        if self.close_at().is_some_and(|due| now() >= due) && !self.pending(&[]) {
            return self.idle_close().await;
        }
        // A working child does not hold the parent's own handover.
        if self.handover_at().is_some_and(|due| now() >= due) && !self.unfinished(&[]) {
            self.handover(Trigger::Idle).await?;
        }
        Ok(())
    }

    /// Marks stalled work from the session's own state, without a queued read keeping it loaded.
    fn stuck(&self, after: Duration) {
        let s = self.record.summary();
        if s.unanswered.is_empty()
            || !s.jobs.is_empty()
            || !s.questions.is_empty()
            || !s.mail_questions.is_empty()
            || now().saturating_sub(s.at) < after.as_millis() as u64
            || self.waiters.iter().any(|(_, waiter)| !waiter.is_closed())
        {
            return;
        }
        let mut status = lock(&self.c.inner.status);
        let descendant = status.agents.values().any(|row| {
            let up = |p: &&SessionId| status.agents.get(*p).and_then(|r| r.parent.as_ref());
            row.state == State::Working
                && std::iter::successors(row.parent.as_ref(), up).any(|p| *p == self.id)
        });
        if let Some(row) = status.agents.get_mut(&self.id)
            && row.state == State::Working
            && !descendant
        {
            row.state = State::Stuck;
            let row = row.clone();
            let _ = status.lines.send(Live::Agent(Box::new(row)));
        }
    }

    /// Idle maintenance remains due: the task stays loaded, and its marker across a restart.
    pub(super) fn due(&self) -> bool {
        !self.blocked
            && !self.record.summary().closed
            && (self.close_at().is_some() || self.handover_at().is_some())
    }

    /// The remaining handover deadline; small and already rebuilt contexts have none, nor does
    /// one whose idle request is already admitted, until external input.
    pub(super) fn handover_at(&self) -> Option<Millis> {
        let s = self.record.summary();
        let at = s.idle_since?;
        let sessions = &self.settings.config.sessions;
        (sessions.handover_minutes > 0.0
            && !s.idle_handover
            && self.context.is_some_and(|n| n >= sessions.handover_tokens)
            && s.last_rebuild.is_none_or(|(_, rebuilt)| rebuilt < at))
        .then_some(at.saturating_add((sessions.handover_minutes * 60_000.0) as Millis))
    }

    /// The remaining close deadline; children and resident homes never have one.
    pub(super) fn close_at(&self) -> Option<Millis> {
        let s = self.record.summary();
        let at = s.idle_since?;
        let config = &self.settings.config;
        let minutes = config.sessions.close_minutes;
        (minutes > 0.0 && s.child.is_none() && !config.residents.contains_key(&self.id))
            .then_some(at.saturating_add((minutes * 60_000.0) as Millis))
    }

    /// Closes only an idle conversation: its handover note reports to its resident without a turn.
    async fn idle_close(&mut self) -> Result<(), Error> {
        let s = self.record.summary();
        let resident = s.thread.as_ref().map(|t| t.resident.clone());
        let mut note = s.last_reply.clone();
        if let Some((id, at)) = s.last_rebuild
            && s.person_input_at.is_none_or(|input| input <= at)
            && let Body::Rebuild { note: saved, .. } = self.record.entry(id).await?.body
            && !saved.is_empty()
        {
            note = saved;
        }
        self.close_out(&[]).await?;
        self.conversation = None;
        let (window, threshold) = limits(&self.model, &self.settings.config, self.reported);
        self.append(
            Body::TurnEnd {
                inputs: Vec::new(),
                end: End::Done,
                usage: Usage::default(),
                context: self.context,
                model: None,
                served_model: None,
                window,
                threshold: Some(threshold),
                closed: true,
                close_reason: Some(CloseReason::Idle),
                report: None,
                child_grace: None,
            },
            None,
        )
        .await?;
        self.set_state(State::Done);
        if let Some(resident) = resident {
            let c = self.c.clone();
            let name = self.id.to_string();
            let input = Input::new(Origin::Program { name }, note);
            tokio::spawn(async move {
                if let Err(e) = c.admit(&resident, input).await {
                    eprintln!("razor: idle conversation report: {e}");
                }
            });
        }
        Ok(())
    }
}
