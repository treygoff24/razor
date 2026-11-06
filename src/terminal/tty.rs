//! The terminal's events, with a report a read split after its ESC put back together: crossterm
//! takes a read that ends in a lone ESC for the Esc key, and the report's rest for typed keys.
//! A full PTY splits a trackpad's burst of wheel reports that way, and can split a terminal's
//! answer to a query.
use crossterm::event::{Event as Input, EventStream, KeyCode, KeyEvent, KeyModifiers as Mods};
use crossterm::event::{MouseButton as B, MouseEvent, MouseEventKind as M};
use futures_util::StreamExt;
use std::{collections::VecDeque, io};
use tokio::time::{Duration, Instant, timeout_at};

/// How long a lone Esc waits for the rest of a split report; no hand types one this fast.
const REST: Duration = Duration::from_millis(50);

/// User input or a complete Kitty answer, without its introducer and terminator.
pub enum Event {
    Input(crossterm::event::Event),
    Picture(String),
}

#[derive(Default)]
pub struct Events {
    stream: EventStream,
    /// An Esc or Alt-_'s arrival, possible report text, and original keys to release on failure.
    /// The report text and keys are bounded by report's 4,096-byte cap.
    held: Option<(Instant, String, Vec<KeyEvent>)>,
    /// An event that ended a held candidate, to examine again after releasing its keys.
    input: Option<io::Result<Input>>,
    ready: VecDeque<io::Result<Event>>,
}

impl Events {
    /// The next event. Dropped unfinished, as `select!` drops it, it keeps what it holds.
    pub async fn next(&mut self) -> Option<io::Result<Event>> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }
            let Some((since, rest, keys)) = &mut self.held else {
                let event = match self.input.take() {
                    Some(event) => event,
                    None => self.stream.next().await?,
                };
                if matches!(&event, Err(e) if e.kind() == io::ErrorKind::Interrupted) {
                    continue;
                }
                let rest = match &event {
                    Ok(Input::Key(k)) if *k == KeyCode::Esc.into() => "",
                    Ok(Input::Key(k)) if *k == KeyEvent::new(KeyCode::Char('_'), Mods::ALT) => "_",
                    _ => return Some(event.map(Event::Input)),
                };
                if let Ok(Input::Key(k)) = event {
                    self.held = Some((Instant::now(), rest.into(), vec![k]));
                }
                continue;
            };
            // Only the Kitty introducer is timed; a confirmed answer may arrive slowly.
            let next = if rest.starts_with("_G") {
                Ok(self.stream.next().await)
            } else {
                timeout_at(*since + REST, self.stream.next()).await
            };
            if matches!(&next, Ok(Some(Err(e))) if e.kind() == io::ErrorKind::Interrupted) {
                continue;
            }
            // An answer's end arrives as Alt-\ (ST), Ctrl-G (BEL), or a read split inside its ST
            // as Esc and then `\`: each kept as the bytes it was.
            let piece = match &next {
                Ok(Some(Ok(Input::Key(k)))) => match (k.code, k.modifiers) {
                    (KeyCode::Esc, Mods::NONE) => Some("\x1b".to_owned()),
                    (KeyCode::Char('\\'), Mods::ALT) => Some("\x1b\\".to_owned()),
                    (KeyCode::Char('g'), Mods::CONTROL) => Some("\x07".to_owned()),
                    (KeyCode::Char(c), Mods::NONE | Mods::SHIFT) => Some(c.to_string()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(piece) = piece
                && let Some(whole) = report(&format!("{rest}{piece}"))
            {
                rest.push_str(&piece);
                if let Ok(Some(Ok(Input::Key(k)))) = next {
                    // A split ESC _ is the same Alt-_ key as an unsplit one.
                    if rest == "_" {
                        keys.clear();
                        keys.push(KeyEvent {
                            modifiers: k.modifiers | Mods::ALT,
                            ..k
                        });
                    } else {
                        keys.push(k);
                    }
                }
                // A whole report is the mouse report or answer it was, or dropped: never the keys
                // it spelled.
                if whole {
                    match rest.chars().next() {
                        Some('[') => self.ready.extend(mouse(rest).map(|e| Ok(Event::Input(e)))),
                        // The `\` of an ST whose ESC a read left alone, as an unsplit one reads.
                        Some('\\') => self.ready.push_back(Ok(Event::Input(Input::Key(st())))),
                        Some('_') => {
                            let body = rest
                                .strip_suffix("\x1b\\")
                                .or_else(|| rest.strip_suffix('\x07'))
                                .unwrap_or(rest);
                            self.ready.push_back(Ok(Event::Picture(body[2..].into())));
                        }
                        _ => self.ready.extend(answer(rest).map(|e| Ok(Event::Input(e)))),
                    }
                    self.held = None;
                }
                continue;
            }
            let keys = self
                .held
                .take()
                .map(|(_, _, keys)| keys)
                .unwrap_or_default();
            self.ready
                .extend(keys.into_iter().map(|k| Ok(Event::Input(Input::Key(k)))));
            self.input = next.ok().flatten();
        }
    }
}

/// Whether the text after a lone ESC is a whole control sequence (`ESC [`, parameters, final
/// byte), answer to a query (`ESC _G` or `ESC ]`, at most 4,096 bytes to ST or BEL) or the rest
/// of an ST (`\`), only the start of one, or neither.
fn report(text: &str) -> Option<bool> {
    if text == "_" {
        return Some(false);
    }
    if text == "\\" {
        return Some(true);
    }
    if let Some(body) = text.strip_prefix("_G").or_else(|| text.strip_prefix(']')) {
        let end = body.ends_with("\x1b\\") || body.ends_with('\x07');
        let open = (body
            .strip_suffix("\x1b\\")
            .or_else(|| body.strip_suffix(['\x1b', '\x07'])))
        .unwrap_or(body);
        return (text.len() <= 4096 && !open.contains(['\x1b', '\x07'])).then_some(end);
    }
    let body = text.strip_prefix('[')?;
    let mut tail = body.trim_start_matches(|c| matches!(c, ' '..='?')).chars();
    match (tail.next(), tail.next()) {
        (None, _) => Some(false),
        (Some('@'..='~'), None) => Some(true),
        _ => None,
    }
}

/// ST as crossterm reads an unsplit one: Alt-\.
fn st() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('\\'), Mods::ALT)
}

/// An answer's keys as crossterm reads an unsplit one: Alt and its intro, a Char per byte, then
/// Alt-\ or Ctrl-G.
fn answer(text: &str) -> impl Iterator<Item = Input> {
    let (body, end) = match text.strip_suffix("\x1b\\") {
        Some(body) => (body, st()),
        None => (
            text.trim_end_matches('\x07'),
            KeyEvent::new(KeyCode::Char('g'), Mods::CONTROL),
        ),
    };
    let mut chars = body.chars();
    let intro = chars
        .next()
        .map(|c| KeyEvent::new(KeyCode::Char(c), Mods::ALT));
    let keys = intro
        .into_iter()
        .chain(chars.map(|c| KeyCode::Char(c).into()));
    keys.chain([end]).map(Input::Key)
}

/// An SGR mouse report, `[<button;column;row` then M pressed or m released, decoded as
/// crossterm decodes one.
fn mouse(report: &str) -> Option<Input> {
    let body = report.strip_prefix("[<")?.strip_suffix(['M', 'm'])?;
    let mut n = body.split(';').map(|n| n.parse::<u16>().ok());
    let (code, column, row) = (n.next()??, n.next()??, n.next()??);
    let b = |n: u16| [B::Left, B::Middle, B::Right][usize::from(n)];
    let scroll = [M::ScrollUp, M::ScrollDown, M::ScrollLeft, M::ScrollRight];
    let kind = match (code & 0b1100_0011, code & 32 != 0) {
        (n @ 0..=2, false) if report.ends_with('m') => M::Up(b(n)),
        (n @ 0..=2, false) => M::Down(b(n)),
        (n @ 0..=2, true) => M::Drag(b(n)),
        (3, false) => M::Up(B::Left),
        (n @ 64..=67, false) => scroll[usize::from(n - 64)],
        _ => return None,
    };
    let bits = [(4, Mods::SHIFT), (8, Mods::ALT), (16, Mods::CONTROL)].into_iter();
    let modifiers = Mods::from_iter(bits.filter_map(|(bit, m)| (code & bit != 0).then_some(m)));
    Some(Input::Mouse(MouseEvent {
        kind,
        column: column.checked_sub(1)?,
        row: row.checked_sub(1)?,
        modifiers,
    }))
}
