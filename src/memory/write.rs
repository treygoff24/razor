//! Memory writes validate provenance, scope and size before changing any file.
use super::{
    Answer, Caller, Controller, Kind, Memory, STATE, Said, TEXT, Write, citations, cite, dreaming,
    files, other, refuse,
};
impl Memory {
    /// A write, a replacement or a retirement, after the rules, all checked before anything is
    /// retired: a ruling's text holds the person's words from the record, a fact or note has a
    /// description, every memory names the entry it comes from, the composed text is at most 16
    /// KiB, and state writes replace this session's note, or the merged note during dreaming.
    /// In a resident's folder, ordinary state writes belong to its home session.
    pub(super) async fn write(
        &self,
        controller: &Controller,
        c: &Caller<'_>,
        mut w: Write,
    ) -> Answer {
        let t = &*TEXT;
        if c.child {
            return Err(refuse(&t.child));
        }
        let summary = controller
            .summary(c.session)
            .await
            .map_err(other("reading the memory writer"))?;
        w.dreaming = dreaming(&summary);
        let old = w.replaces.as_deref().unwrap_or_default();
        if !w.dreaming && (w.kind == Some(Kind::Page) || old.starts_with("wiki/")) {
            return Err(refuse(&t.page_only));
        }
        if w.dreaming
            && (w.kind == Some(Kind::Ruling) || files::kind(old) == "ruling")
            && !(c.answer && !old.is_empty() && w.text.is_empty())
        {
            return Err(refuse(&t.dream_ruling));
        }
        if w.text.is_empty() {
            let id = w.replaces.ok_or_else(|| refuse(&t.no_text))?;
            let lookup = id.clone();
            let all = self
                .files(controller, c.cwd, move |p| {
                    files::global(&p.memory, &lookup)
                })
                .await?;
            return self
                .memory_change(controller, c.cwd, all, move |p| {
                    files::retire(&p.memory, &id, false)
                })
                .await;
        }
        let kind = w.kind.ok_or_else(|| refuse(&t.kind))?;
        if kind == Kind::Page && !w.name.as_deref().is_some_and(files::page_name) {
            return Err(refuse(&t.page_name));
        }
        match kind {
            Kind::State if w.all => return Err(refuse(&t.state_all)),
            Kind::State if w.text.len() > STATE => return Err(refuse(&t.state_long)),
            Kind::Fact | Kind::Note | Kind::Page => {
                let line = w.description.as_deref().unwrap_or_default();
                if line.trim().is_empty() || line.len() > 200 || line.contains(['\n', '\r']) {
                    return Err(refuse(&t.description));
                }
            }
            // A nonempty state write replaces its own file, not a caller-named memory.
            Kind::State => (w.description, w.replaces) = (None, None),
            _ => w.description = None,
        }
        if w.cites.len() > 32 || w.cites.iter().any(|c| !citations::valid(c)) {
            return Err(refuse(&t.cites));
        }
        let date = |d: String| d.parse::<jiff::civil::Date>().map(|d| d.to_string());
        let checked = w.checked.map(date).transpose();
        w.checked = checked.map_err(|_| refuse(&t.expires))?;
        let expires = w.expires.map(date).transpose();
        w.expires = expires.map_err(|_| refuse(&t.expires))?;
        w.source = format!("razor:{}:l-{}", c.session, c.at.0);
        w.session = c.session.to_string();
        if kind == Kind::State && !w.dreaming && w.expires.is_none() {
            let until = jiff::Zoned::now()
                .date()
                .checked_add(jiff::Span::new().days(14));
            w.expires = Some(until.map_err(other("dating a state note"))?.to_string());
        }
        let ruled = (kind == Kind::Ruling)
            .then(|| cite(c, w.cite))
            .transpose()?;
        if let Some((cited, Said { words, asked })) = ruled {
            // The person's words and the question they answer, verbatim, then the agent's.
            w.source = format!("razor:{}:l-{}", c.session, cited.0);
            let q = format!("the agent asked: \"{asked}\"\n\n");
            let q = if asked.is_empty() { "" } else { &q };
            let said = format!("{q}the human said ({}): \"{words}\"\n\n", w.source);
            w.text = format!("{said}agent's reading: {}", w.text);
        }
        if w.text.len() > 16 * 1024 {
            return Err(refuse(&t.too_long));
        }
        let session = c.session.clone();
        self.memory_change(controller, c.cwd, w.all, move |p| {
            if kind == Kind::State
                && !w.dreaming
                && let Some(home) = p.resident.clone().filter(|r| *r != session)
            {
                let state = files::state(&p.memory).unwrap_or_else(|| TEXT.no_state.clone());
                let (thread, heading) = (&TEXT.thread, &TEXT.state);
                return Err(refuse(&format!("{thread} {home}.\n\n{heading}\n\n{state}")));
            }
            files::write(&p.memory, kind, w.clone())
        })
        .await
    }
}
