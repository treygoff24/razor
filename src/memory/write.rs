//! Memory writes validate provenance, scope and size before changing any file.
use super::{
    Answer, Caller, Kind, Memory, Project, STATE, Summary, TEXT, Write, blocking, citations, cite,
    dreaming, files, other, own, refuse,
};
impl Memory {
    /// A write, a replacement or a retirement, after the rules, all checked before anything is
    /// retired: a ruling's text holds the person's words from the record, a fact or note has a
    /// description, every memory names the entry it comes from, the composed text is at most 16
    /// KiB, and state writes replace this session's note, or the merged note during dreaming.
    /// A resident's home session writes its state into its own layer, and a thread writes none.
    pub(super) async fn write(
        &self,
        c: &Caller<'_>,
        p: Project,
        s: &Summary,
        resident: bool,
        mut w: Write,
    ) -> Answer {
        let t = &*TEXT;
        if c.child {
            return Err(refuse(&t.child));
        }
        w.dreaming = dreaming(s);
        let (state, own) = (w.kind == Some(Kind::State) && !w.dreaming, own(&p, s));
        if let Some(home) = own.as_ref().filter(|_| state && s.thread.is_some()) {
            return Err(refuse(&format!("{} {home}.", t.thread)));
        }
        let layer = own.as_deref().filter(|_| resident || state);
        let Some(kind) = w.prepare(c)? else {
            let ids = w.replaces.ok_or_else(|| refuse(&t.no_text))?;
            let lookup = ids.ids().first().ok_or_else(|| refuse(&t.no_text))?.clone();
            let memory = p.memory.layer(layer);
            let all = blocking(move || files::global(&memory, &lookup)).await?;
            let retire = move |m: &files::Folders| files::retire(m, ids.ids(), false);
            return self.memory_change(&p, all, layer, retire).await;
        };
        let all = w.all;
        let write = move |m: &files::Folders| files::write(m, kind, w.clone());
        self.memory_change(&p, all, layer, write).await
    }
}

impl Write {
    /// Validates before either branch changes files; only a nonempty write copies its source.
    fn prepare(&mut self, c: &Caller<'_>) -> Result<Option<Kind>, super::Error> {
        let t = &*TEXT;
        let old = self.replaces.as_ref().map_or(&[][..], |r| r.ids());
        let ruling = self.kind == Some(Kind::Ruling);
        let fresh = old.is_empty() || self.cite.is_some() || !self.settles.is_empty();
        let rulings = old.iter().filter(|id| files::kind(id) == "ruling").count();
        let incompatible = (ruling && rulings != old.len()) || (!ruling && rulings > 0);
        if !self.dreaming
            && (self.kind == Some(Kind::Page) || old.iter().any(|id| id.starts_with("wiki/")))
        {
            return Err(refuse(&t.page_only));
        }
        if old.len() > 32
            || old.iter().enumerate().any(|(i, id)| old[..i].contains(id))
            || (old.len() > 1 && (rulings != old.len() || (!self.text.is_empty() && incompatible)))
        {
            return Err(refuse(&t.replacements));
        }
        if self.dreaming && ruling && (fresh || rulings != old.len()) {
            return Err(refuse(&t.dream_ruling));
        }
        if !self.settles.is_empty() && (!ruling || self.text.is_empty()) {
            return Err(refuse(&t.settles));
        }
        if self.text.is_empty() {
            return Ok(None);
        }
        let kind = self.kind.ok_or_else(|| refuse(&t.kind))?;
        if incompatible {
            return Err(refuse(&t.replacements));
        }
        if kind == Kind::Page && !self.name.as_deref().is_some_and(files::page_name) {
            return Err(refuse(&t.page_name));
        }
        match kind {
            Kind::Ruling if self.expires.is_some() => return Err(refuse(&t.ruling_expires)),
            Kind::Ruling if self.text.trim().is_empty() || self.text.contains(['\n', '\r']) => {
                return Err(refuse(&t.ruling_line));
            }
            Kind::State if self.all => return Err(refuse(&t.state_all)),
            Kind::State if self.text.len() > STATE => return Err(refuse(&t.state_long)),
            Kind::Fact | Kind::Note | Kind::Page => {
                let line = self.description.as_deref().unwrap_or_default();
                if line.trim().is_empty() || line.len() > 200 || line.contains(['\n', '\r']) {
                    return Err(refuse(&t.description));
                }
            }
            // A nonempty state write replaces its own file, not a caller-named memory.
            Kind::State => (self.description, self.replaces) = (None, None),
            _ => self.description = None,
        }
        if self.cites.len() > 32 || self.cites.iter().any(|c| !citations::valid(c)) {
            return Err(refuse(&t.cites));
        }
        let date = |d: String| d.parse::<jiff::civil::Date>().map(|d| d.to_string());
        let checked = self.checked.take().map(date).transpose();
        self.checked = checked.map_err(|_| refuse(&t.expires))?;
        let expires = self.expires.take().map(date).transpose();
        self.expires = expires.map_err(|_| refuse(&t.expires))?;
        self.source = format!("razor:{}:l-{}", c.session, c.at.0);
        self.session = c.session.to_string();
        if kind == Kind::State && !self.dreaming && self.expires.is_none() {
            let until = jiff::Zoned::now()
                .date()
                .checked_add(jiff::Span::new().days(14));
            self.expires = Some(until.map_err(other("dating a state note"))?.to_string());
        }
        let ruled = (ruling && fresh).then(|| cite(c, self.cite)).transpose()?;
        if let Some((cited, words)) = ruled {
            self.source = format!("razor:{}:l-{}", c.session, cited.0);
            self.quotes.push((self.source.clone(), words.to_owned()));
        }
        if self.text.len() > 16 * 1024 {
            return Err(refuse(&t.too_long));
        }
        Ok(Some(kind))
    }
}
