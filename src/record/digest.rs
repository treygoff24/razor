//! A bounded readout from one sequential record scan, including children and fresh runs.

use super::{Error, blocking, damaged, io, search};
use crate::protocol::{Body, Entry, EntryId, Outcome, SessionId};
use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};
use std::{io::BufRead, io::Read, path::Path};

/// Inputs, replies, failed calls, memory/task calls and changed files, in entry order.
/// At most `bytes`: newest failures have priority, then newest other entries; omitted counts
/// end the readout. One sequential read through the length at open; a torn tail is ignored.
pub(crate) async fn digest(
    home: &Path,
    session: &SessionId,
    bytes: usize,
) -> Result<String, Error> {
    let (home, session) = (home.to_owned(), session.clone());
    blocking(move || {
        let path = home
            .join("sessions")
            .join(session.as_str())
            .join("record.jsonl");
        let file = std::fs::File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::Missing(session.clone()),
            _ => io("opening a digest's record")(e),
        })?;
        let length = file
            .metadata()
            .map_err(io("reading record metadata"))?
            .len();
        let mut reader = std::io::BufReader::with_capacity(64 * 1024, file.take(length));
        let mut kept = Kept::new(bytes.saturating_sub(128)); // Room for both omitted counts.
        // The model step's unresolved calls, as in Summary; only names and short targets.
        let mut calls = BTreeMap::<String, (String, String)>::new();
        let mut line = Vec::new(); // One stored entry, as in cursor::fold, never the record.
        let mut next = 1;
        loop {
            line.clear();
            reader
                .read_until(b'\n', &mut line)
                .map_err(io("reading a digest"))?;
            if line.last() != Some(&b'\n') {
                break;
            }
            // Skip native replay data without allocating it or decoding unused entry bodies.
            let header: Header<'_> = serde_json::from_slice(&line)
                .map_err(|e| damaged(&session, next, e.to_string()))?;
            if header.id.0 != next || (header.kind == "start") != (next == 1) {
                return Err(damaged(
                    &session,
                    next,
                    "its id or start is misplaced".into(),
                ));
            }
            next += 1;
            if !matches!(
                header.kind,
                "input" | "text" | "call" | "result" | "changed_files"
            ) {
                continue;
            }
            let entry: Entry = serde_json::from_slice(&line)
                .map_err(|e| damaged(&session, header.id.0, e.to_string()))?;
            let Some((failed, text)) = describe(&entry.body, &mut calls) else {
                continue;
            };
            let text = clip(
                &format!("#{} {text}", entry.id.0),
                1024 - 1, // Each line is at most 1 KiB, including its newline.
            ) + "\n";
            kept.push(entry.id, failed, text);
        }
        if next == 1 {
            return Err(damaged(&session, 1, super::EMPTY.into()));
        }
        Ok(clip(&kept.finish(), bytes))
    })
    .await?
}

/// Calls retain only the same short target used in summaries, until their result arrives.
fn describe(body: &Body, calls: &mut BTreeMap<String, (String, String)>) -> Option<(bool, String)> {
    if let Some((from, text)) = search::text(body) {
        return Some((false, format!("{from}: {text}")));
    }
    let failed = matches!(
        body,
        Body::Result {
            outcome: Outcome::Error,
            ..
        }
    );
    let text = match body {
        Body::Call { call, name, input } => {
            calls.insert(call.0.clone(), (name.clone(), super::target(input)));
            let name = super::action(name);
            if !matches!(name, "memory" | "task") {
                return None;
            }
            format!("{name} {input}")
        }
        Body::Result { call, output, .. } => {
            let called = calls.remove(&call.0);
            let (name, target) = called
                .as_ref()
                .map_or(("", ""), |(n, t)| (super::action(n), t.as_str()));
            if failed {
                format!(
                    "failed {name} {target} ({}): {}",
                    call.0,
                    output.lines().next().unwrap_or_default()
                )
            } else {
                return None;
            }
        }
        Body::ChangedFiles { files } => format!("changed files: {}", serde_json::json!(files)),
        _ => return None,
    };
    Some((failed, text))
}

#[derive(Deserialize)]
struct Header<'a> {
    id: EntryId,
    kind: &'a str,
}

/// A clipped line keeps its bound and a marker, without splitting a UTF-8 character.
fn clip(text: &str, bytes: usize) -> String {
    const MARK: &str = "… [clipped]";
    if text.len() <= bytes {
        return text.into();
    }
    let end = text.floor_char_boundary(bytes.saturating_sub(MARK.len()));
    let mark = &MARK[..MARK.floor_char_boundary(bytes.min(MARK.len()))];
    format!("{}{mark}", &text[..end])
}

/// Each queue holds at most `limit` bytes; turns survive until the final failure budget is known.
struct Kept {
    limit: usize,
    failures: VecDeque<(EntryId, String)>,
    turns: VecDeque<(EntryId, String)>,
    sizes: [usize; 2],
    omitted: [u64; 2],
}

impl Kept {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            failures: VecDeque::new(),
            turns: VecDeque::new(),
            sizes: [0; 2],
            omitted: [0; 2],
        }
    }

    fn push(&mut self, id: EntryId, failed: bool, text: String) {
        let (queue, n) = if failed {
            (&mut self.failures, 0)
        } else {
            (&mut self.turns, 1)
        };
        self.sizes[n] += text.len();
        queue.push_back((id, text));
        while self.sizes[n] > self.limit {
            if let Some((_, text)) = queue.pop_front() {
                self.sizes[n] -= text.len();
                self.omitted[n] += 1;
            }
        }
    }

    fn finish(mut self) -> String {
        while self.sizes[0] + self.sizes[1] > self.limit {
            if let Some((_, text)) = self.turns.pop_front() {
                self.sizes[1] -= text.len();
                self.omitted[1] += 1;
            }
        }
        let mut lines: Vec<_> = self.failures.into_iter().chain(self.turns).collect();
        lines.sort_unstable_by_key(|(id, _)| id.0);
        let mut text: String = lines.into_iter().map(|(_, line)| line).collect();
        if self.omitted != [0; 2] {
            text += &format!(
                "({} failures and {} other entries left out.)\n",
                self.omitted[0], self.omitted[1]
            );
        }
        text
    }
}
