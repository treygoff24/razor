//! A reply's optional last line proposing the person's next message:
//! the record keeps it, and every reader a person sees gets the reply without it.

use crate::protocol::{Body, Entry};
use icu_properties::{CodePointMapData, props::GeneralCategory};
use serde::Deserialize;

/// What begins the line, at the start of the reply's last line and outside a code fence.
const MARKER: &str = "[razor-suggestion]";
/// The longest such line in bytes, which is all a stream ever holds back of a reply.
const LINE: usize = 1024;

/// A reply as a person reads it.
pub(crate) struct Reply<'a> {
    /// The whole answer without a valid suggestion line.
    pub(crate) text: &'a str,
    /// The proposed next message; never an input until the person sends it.
    pub(crate) suggestion: Option<String>,
}

/// Splits one complete reply; a marker that is malformed, not last, quoted or fenced stays text,
/// as does one whose line and the blank text after it pass 1 KiB.
pub(crate) fn reply(text: &str) -> Reply<'_> {
    let body = text.trim_end();
    let start = body.rfind('\n').map_or(0, |n| n + 1);
    let fits = text.len() - start <= LINE;
    let fenced = || (body[..start].lines()).fold(None, fence);
    match suggestion(&body[start..]).filter(|_| fits && fenced().is_none()) {
        Some(s) => Reply {
            text: body[..start].trim_end(),
            suggestion: Some(s),
        },
        None => Reply {
            text,
            suggestion: None,
        },
    }
}

/// The suggestion a marked line holds: JSON with only `text`, 1 to 120 characters with no
/// control characters, not blank and not a slash command.
fn suggestion(line: &str) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Line {
        text: String,
    }
    let json = line.strip_prefix(MARKER)?;
    let text = serde_json::from_str::<Line>(json.trim()).ok()?.text;
    let words = text.trim();
    let categories = CodePointMapData::<GeneralCategory>::new();
    let fits = (1..=120).contains(&text.chars().count())
        && !text.chars().any(|c| {
            c.is_control()
                || matches!(c, '\u{2028}' | '\u{2029}')
                || categories.get(c) == GeneralCategory::Format
        });
    (fits && !words.is_empty() && !words.starts_with('/')).then(|| words.to_owned())
}

/// The CommonMark fence open after this line, if any.
fn fence(open: Option<(u8, usize)>, line: &str) -> Option<(u8, usize)> {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|&&b| b == b' ').count();
    let Some(&c @ (b'`' | b'~')) = bytes.get(indent).filter(|_| indent <= 3) else {
        return open;
    };
    let n = bytes[indent..].iter().take_while(|&&b| b == c).count();
    let rest = &bytes[indent + n..];
    match open {
        None if n >= 3 && (c == b'~' || !rest.contains(&b'`')) => Some((c, n)),
        Some((o, m)) if c == o && n >= m && rest.iter().all(u8::is_ascii_whitespace) => None,
        _ => open,
    }
}

/// A reply's live words with the line that may yet be its suggestion held back, so a stream
/// never shows one. The reply's entry, which replaces the live words, shows what was held when
/// it was not a suggestion; anything that proves it is not shows it at once. It keeps at most
/// 1 KiB of the line being streamed and 1 KiB of held words.
#[derive(Debug, Default)]
pub(crate) struct Hold {
    /// The line being streamed, without its line break, while it is at most a byte past 1 KiB,
    /// room for a `\r`; past that it is `long`, and not a marker.
    line: String,
    long: bool,
    /// Held words: a marked line and the blank text after it, or the current line.
    held: String,
    /// `held` starts with a complete, valid marked line, followed only by blank text.
    marked: bool,
    /// Words of this reply have been shown.
    begun: bool,
}

impl Hold {
    /// The words of `delta` to show now, after any held ones that proved not to be a suggestion,
    /// and whether they are the first shown.
    pub(crate) fn push(&mut self, delta: &str) -> (String, bool) {
        let mut shown = String::new();
        for piece in delta.split_inclusive('\n') {
            let (words, done) = (piece.trim_end_matches('\n'), piece.ends_with('\n'));
            // A line is held from its start, or not at all.
            let mut holding = !self.held.is_empty();
            if self.line.is_empty() && !self.long {
                // A line with words after a marked line shows that line was not the last.
                if self.marked && !piece.trim().is_empty() {
                    shown.push_str(&std::mem::take(&mut self.held));
                    self.marked = false;
                }
                holding = true;
            }
            self.long |= self.line.len() + words.len() > LINE + 1;
            if !self.long {
                self.line.push_str(words);
            }
            let line = (!self.long).then(|| self.line.strip_suffix('\r').unwrap_or(&self.line));
            let marker = line.is_some_and(|l| l.starts_with(MARKER));
            holding &= self.held.len() + piece.len() <= LINE
                && line.is_some_and(|l| match marker {
                    true => !done || suggestion(l.trim_end()).is_some(),
                    false => !done && MARKER.starts_with(l) || self.marked && l.trim().is_empty(),
                });
            if holding {
                self.held.push_str(piece);
                self.marked |= done && marker;
            } else {
                shown.push_str(&std::mem::take(&mut self.held));
                shown.push_str(piece);
                self.marked = false;
            }
            if done {
                (self.line, self.long) = (String::new(), false);
            }
        }
        let start = !(shown.is_empty() || std::mem::replace(&mut self.begun, true));
        (shown, start)
    }
}

/// A recorded `Text` entry's line as a person reads it, its suggestion split out and its
/// `native` left out; `None` for any other line, or a reply without a suggestion.
pub(crate) fn shown(line: &str) -> Option<String> {
    let line = Some(line).filter(|line| line.contains(MARKER))?;
    let mut entry: Entry = serde_json::from_str(line).ok()?;
    let Body::Text { text } = &mut entry.body else {
        return None;
    };
    let Reply {
        text: kept,
        suggestion,
    } = reply(text);
    (entry.suggestion, entry.native) = (Some(suggestion?), None);
    text.truncate(kept.len());
    serde_json::to_string(&entry).ok()
}
