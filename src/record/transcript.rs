//! Transient search of one record, using its indexed pages and the shared word matcher.
use super::{Error, Replay, Summary, search};
use crate::protocol::{Body, CallId, End, EntryId, Hit, Origin, SessionId};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// At most 50 matches in occurrence order; pending results and answers await their Call.
pub(crate) struct TranscriptSearch {
    words: Vec<String>,
    limit: usize,
    hits: Vec<(Hit, Link)>,
    withdrawn: HashSet<EntryId>,
    template: Hit,
    children: HashMap<SessionId, String>,
}

impl TranscriptSearch {
    pub(crate) fn new(words: Vec<String>, limit: usize, s: Summary) -> Self {
        Self {
            words,
            limit: limit.clamp(1, 50),
            hits: Vec::new(),
            withdrawn: HashSet::new(),
            children: s
                .children
                .into_iter()
                .map(|c| (c.session, c.child.name))
                .collect(),
            template: Hit {
                session: s.session,
                name: s.name,
                cwd: s.cwd,
                resident: s.thread.as_ref().map(|t| t.resident.clone()),
                resident_project: s.thread.as_ref().and_then(|t| t.resident_project.clone()),
                at: 0,
                entry: EntryId(0),
                from: String::new(),
                snippet: String::new(),
            },
        }
    }

    /// Decode one line at a time off the async workers, then consume compact facts newest first.
    pub(crate) async fn page(mut self, mut page: Replay) -> Result<(Self, Option<EntryId>), Error> {
        let runtime = tokio::runtime::Handle::current();
        super::blocking(move || {
            let mut facts = Vec::new();
            while let Some(line) = runtime.block_on(page.next_line()) {
                let line = line?;
                let entry: Stored = serde_json::from_str(&line).map_err(|e| {
                    super::damaged(&self.template.session, page.line, e.to_string())
                })?;
                facts.push(self.project(entry));
            }
            let before = facts.first().map(|f| f.id);
            for fact in facts.into_iter().rev() {
                self.consume(fact);
            }
            let pending = self
                .hits
                .iter()
                .find(|(_, link)| !matches!(link, Link::Row(_)));
            if before.is_none_or(|id| id.0 <= 1)
                && let Some((hit, _)) = pending
            {
                return Err(super::damaged(
                    &self.template.session,
                    hit.entry.0,
                    "a matching result or answer has no call".into(),
                ));
            }
            let more =
                before.filter(|id| id.0 > 1 && (self.hits.len() < self.limit || pending.is_some()));
            Ok((self, more))
        })
        .await?
    }

    pub(crate) fn finish(self) -> Vec<Hit> {
        self.hits.into_iter().map(|(hit, _)| hit).collect()
    }

    fn project(&self, e: Stored) -> Fact {
        let (mut text, mut call, mut link) = (String::new(), None, Link::Row(e.id));
        let mut withdrawn = Vec::new();
        let (from, text) = match &e.body {
            Body::Call {
                call: id,
                name,
                input,
            } => {
                call = Some((id.clone(), name.clone()));
                text.clone_from(name);
                strings(input, &mut text);
                (name.clone(), text)
            }
            Body::Result {
                call: id, output, ..
            } => {
                link = Link::Call(id.clone());
                match serde_json::from_str::<serde_json::Value>(output) {
                    Ok(v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => {
                        strings(&v, &mut text)
                    }
                    _ => text.clone_from(output),
                }
                (String::new(), text)
            }
            Body::Input(input) => {
                let (from, _) = search::text(&e.body).unwrap_or_else(|| match &input.from {
                    Origin::Program { name } | Origin::Schedule { name, .. } => (name, ""),
                    _ => ("Razor", ""),
                });
                let from = match &input.from {
                    Origin::Answer { ask } => {
                        link = Link::Ask(*ask);
                        from
                    }
                    Origin::Report { child, .. } => {
                        self.children.get(child).map_or(from, String::as_str)
                    }
                    _ => from,
                };
                (
                    from.into(),
                    super::text::skill(input.skill.as_ref(), input.text.clone()),
                )
            }
            Body::Text { .. } => {
                let (from, text) = search::text(&e.body).expect("reply text");
                (from.into(), text.into())
            }
            Body::Thinking { text } => ("Thinking".into(), text.clone()),
            Body::Notice { text } => ("notice".into(), text.clone()),
            Body::Rebuild { note, .. } => ("rebuild".into(), note.clone()),
            Body::TurnEnd {
                end: End::Error(error),
                ..
            } => ("error".into(), error.clone()),
            Body::Withdrawn { inputs } => {
                withdrawn.clone_from(inputs);
                (String::new(), text)
            }
            _ => (String::new(), text),
        };
        let folded = search::fold(&text);
        let hit = self
            .words
            .iter()
            .all(|w| search::begins(folded.as_bytes(), w).is_some())
            .then(|| Hit {
                entry: e.id,
                at: e.at,
                from,
                snippet: search::snippet(folded.as_bytes(), text.as_bytes(), &self.words)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                ..self.template.clone()
            });
        Fact {
            id: e.id,
            hit,
            call,
            link,
            withdrawn,
        }
    }

    fn consume(&mut self, f: Fact) {
        self.withdrawn.extend(f.withdrawn);
        if self.withdrawn.remove(&f.id) {
            return;
        }
        let full = self.hits.len() == self.limit;
        let mut resolved = false;
        self.hits.retain_mut(|(hit, link)| {
            if *link != Link::Ask(f.id)
                && !matches!((&link, &f.call), (Link::Call(a), Some((b, _))) if a == b)
            {
                return true;
            }
            let Some((_, name)) = &f.call else {
                return false;
            };
            if resolved {
                return false;
            }
            resolved = true;
            hit.entry = f.id;
            if matches!(link, Link::Call(_)) {
                hit.from.clone_from(name);
            }
            *link = Link::Row(f.id);
            true
        });
        self.limit = if full { self.hits.len() } else { self.limit };
        if self.hits.len() < self.limit
            && let Some(hit) = f.hit
            && !self.hits.iter().any(|(_, link)| *link == f.link)
        {
            self.hits.push((hit, f.link));
        }
    }
}

#[derive(PartialEq)]
enum Link {
    Row(EntryId),
    Call(CallId),
    Ask(EntryId),
}
#[derive(Deserialize)]
struct Stored<'a> {
    id: EntryId,
    at: u64,
    // Consume native as raw JSON before flattening Body, without decoding its private data.
    #[serde(borrow, rename = "native")]
    _native: Option<&'a serde_json::value::RawValue>,
    #[serde(flatten)]
    body: Body,
}
struct Fact {
    id: EntryId,
    hit: Option<Hit>,
    call: Option<(CallId, String)>,
    link: Link,
    withdrawn: Vec<EntryId>,
}
fn strings(value: &serde_json::Value, text: &mut String) {
    use serde_json::Value;
    match value {
        Value::String(s) => {
            text.push(' ');
            text.push_str(s);
        }
        Value::Array(a) => a.iter().for_each(|v| strings(v, text)),
        Value::Object(o) => o.values().for_each(|v| strings(v, text)),
        _ => {}
    }
}
