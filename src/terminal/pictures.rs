//! Pictures in the transcript. A session's image file is read off the loop in byte ranges,
//! screened, and decoded once within fixed bounds into a small thumbnail the transcript draws as
//! half-block cells. Where the terminal answered Kitty's upload probe, a frame that draws the
//! picture has it read again into a copy at the pixels it is drawn at, sent as one complete Kitty
//! image per cell row, top down, each once the terminal has answered the last, so that at most a
//! row is on its way; each row answered is placed over its ground each frame the cells are still
//! the picture's own.
use super::{App, Transcript, api::Api, transcript::Row};
use crate::protocol::SessionId;
use crate::terminal::image_codec::{PIXELS, SENT, THUMB, decode};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::RgbaImage;
use ratatui::{buffer::Buffer, buffer::Cell, layout::Rect, style::Color};
use std::{collections::BTreeMap, path::Path, sync::Arc};
use tokio::time::{Duration, Instant};

// A file is read whole, at most 32 MiB, in ranges of 4 MiB.
const ENCODED: u64 = 32 << 20;
const RANGE: u64 = 4 << 20;
// All transcripts' thumbnails together hold at most 8 MiB.
const THUMBS: usize = 8 << 20;
// Kitty's copy of a picture takes at most its own pixels or 2048×2048, the more of the two; the
// terminal holds at most 64 pictures and 256 MiB of their pixels for this app.
const HELD: u64 = 256 << 20;

/// A transcript's pictures by session file, each with the frame that last showed or asked for
/// it, and none while it is read or when it was refused, which keeps its chip: at most 256 across
/// every transcript, their thumbnails within 8 MiB.
pub(super) type Known = BTreeMap<Arc<str>, (u64, Option<Picture>)>;

/// A picture's size in pixels and its thumbnail.
pub(super) type Picture = ((u32, u32), RgbaImage);

/// A transcript row's part of a picture: its file, which of its rows, and its size in cells.
#[derive(Clone)]
pub(super) struct Slice {
    pub(super) name: Arc<str>,
    pub(super) row: u16,
    pub(super) cells: (u16, u16),
}

/// The rows of a picture one frame drew: where they start, its first row there and how many,
/// the columns drawn, its size in cells, and the cells as drawn, row by row.
pub(super) struct Fragment {
    pub(super) name: Arc<str>,
    pub(super) at: (u16, u16),
    pub(super) row: u16,
    pub(super) rows: u16,
    pub(super) width: u16,
    pub(super) cells: (u16, u16),
    pub(super) ground: Color,
    pub(super) drawn: Vec<Cell>,
}

/// What sizes a transcript's pictures: those known, the rows one may take and a cell's pixels.
#[derive(Clone, Copy)]
pub(super) struct Room<'a> {
    pub(super) known: &'a Known,
    pub(super) rows: u16,
    pub(super) cell: (u16, u16),
}

impl Room<'_> {
    /// The rows of the picture at `path` from column `lead` on `bg`, within `columns`: none until
    /// its pixels are known, or when it would take under 6 columns or 3 rows.
    pub(super) fn rows(&self, path: &Path, lead: u16, columns: usize, bg: Color) -> Vec<Row> {
        let Some(name) = name(path) else {
            return Vec::new();
        };
        let Some((_, Some((size, _)))) = self.known.get(&name) else {
            return Vec::new();
        };
        let Some(cells) = cells(*size, columns, self.rows, self.cell) else {
            return Vec::new();
        };
        let blank = Row::on(" ".repeat(usize::from(lead)), bg).frame();
        let slice = |row| Slice {
            name: name.clone(),
            row,
            cells,
        };
        let row = |row| Row {
            picture: Some(slice(row)),
            lead,
            ..blank.clone()
        };
        (0..cells.1).map(row).collect()
    }
}

// A w×h picture's columns and rows within `columns` and `rows`, its aspect kept on cells of
// `cell` pixels: at its own size, enlarged whole, or smaller to fit; none under 6 by 3.
fn cells(size: (u32, u32), columns: usize, rows: u16, cell: (u16, u16)) -> Option<(u16, u16)> {
    let n = f64::from(enlarge(size, cell));
    let w = f64::from(size.0) * n / f64::from(cell.0.max(1));
    let h = f64::from(size.1) * n / f64::from(cell.1.max(1));
    let k = (columns as f64 / w).min(f64::from(rows) / h).min(1.);
    let (c, r) = ((w * k).floor(), (h * k).floor());
    (c >= 6. && r >= 3.).then_some((c as u16, r as u16))
}

/// The least whole factor that enlarges a w×h picture to at least 6 columns by 3 rows of `cell`
/// pixels, each of its pixels a block of the next, within SENT a side; 1 for a picture that size
/// or larger.
fn enlarge((w, h): (u32, u32), (cw, ch): (u16, u16)) -> u32 {
    let up = |px: u32, least: u32| least.div_ceil(px.max(1));
    let n = up(w, 6 * u32::from(cw)).max(up(h, 3 * u32::from(ch)));
    // Never past SENT a side, which bounds the enlarged copy Kitty receives.
    n.min(SENT / w.max(h).max(1)).max(1)
}

/// The session file a picture's path names, `images/<file>`: an input's relative path or a
/// result's absolute one, in the session's `images` folder; none for any other path.
pub(super) fn name(path: &Path) -> Option<Arc<str>> {
    let file = path.file_name()?.to_str()?;
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"._-".contains(&b);
    let safe = !file.starts_with('.') && file.bytes().all(plain);
    (safe && path.parent()?.file_name()? == "images").then(|| format!("images/{file}").into())
}

/// The cell for two stacked pixels: a space when they match, else a half block, a missing
/// pixel showing `ground`; none when both are missing. The stage's octant fallback shares it.
pub(super) fn half(
    top: Option<[u8; 3]>,
    bottom: Option<[u8; 3]>,
    ground: [u8; 3],
) -> Option<(char, [u8; 3], [u8; 3])> {
    Some(match (top, bottom) {
        (Some(t), Some(b)) if t == b => (' ', t, t),
        (Some(t), Some(b)) => ('▀', t, b),
        (Some(t), None) => ('▀', t, ground),
        (None, Some(b)) => ('▄', b, ground),
        (None, None) => return None,
    })
}

/// Draws row `slice.row` of a picture from its thumbnail as half-block cells along `r`'s top
/// row, clipped to `r`, its transparency over `ground`.
pub(super) fn paint(b: &mut Buffer, r: Rect, thumb: &RgbaImage, slice: &Slice, ground: Color) {
    let Color::Rgb(red, green, blue) = ground else {
        return;
    };
    let ground = [red, green, blue];
    let (columns, rows) = (u32::from(slice.cells.0), 2 * u32::from(slice.cells.1));
    let y = 2 * u32::from(slice.row);
    for x in 0..r.width.min(slice.cells.0) {
        let pixel = |y| sample(thumb, (u32::from(x), columns), (y, rows), ground);
        let cell = half(Some(pixel(y)), Some(pixel(y + 1)), ground);
        if let (Some((glyph, fg, bg)), Some(c)) = (cell, b.cell_mut((r.x + x, r.y))) {
            let rgb = |c: [u8; 3]| Color::Rgb(c[0], c[1], c[2]);
            c.set_char(glyph).set_fg(rgb(fg)).set_bg(rgb(bg));
        }
    }
}

// The mean of the thumbnail's pixels under sample `x` of `w` across and `y` of `h` down, its
// transparency over `ground`.
fn sample(t: &RgbaImage, (x, w): (u32, u32), (y, h): (u32, u32), ground: [u8; 3]) -> [u8; 3] {
    let span =
        |i: u32, n: u32, size: u32| (i * size / n, ((i + 1) * size / n).max(i * size / n + 1));
    let ((x0, x1), (y0, y1)) = (span(x, w, t.width()), span(y, h, t.height()));
    let mut sum = [0u32; 4];
    for (px, py) in (y0..y1).flat_map(|py| (x0..x1).map(move |px| (px, py))) {
        let p = t.get_pixel(px, py).0;
        for c in 0..3 {
            sum[c] += u32::from(p[c]) * u32::from(p[3]);
        }
        sum[3] += u32::from(p[3]);
    }
    let n = (x1 - x0) * (y1 - y0) * 255;
    std::array::from_fn(|c| ((sum[c] + u32::from(ground[c]) * (n - sum[3])) / n) as u8)
}

/// The terminal's side of pictures: its answer to the upload probe, the one read under way,
/// what the terminal holds for this app, the row it has yet to answer, and the last frame's
/// placements.
#[derive(Default)]
pub(super) struct Pictures {
    /// A cell's pixels, as the terminal last reported its size.
    pub(super) cell: (u16, u16),
    /// The frame being drawn, which stamps the pictures it shows.
    pub(super) frame: u64,
    // None until the terminal answers the multipart upload probe.
    kitty: Option<bool>,
    probe: Option<u32>,
    loading: bool,
    last: u32,
    // At most 64 pictures and 256 MiB of their pixels, by picture.
    held: BTreeMap<Key, Held>,
    // The picture whose rows are being sent and its copy.
    sending: Option<(Key, RgbaImage)>,
    // The row sent and not yet answered, by client id, and when the terminal took all of it.
    waiting: Option<(u32, Option<Instant>)>,
    // The longest the terminal has taken to answer a row.
    slowest: Duration,
    // Deletes of pictures no longer held, written before the next row or frame.
    deletes: String,
    placed: Vec<Placement>,
}

type Key = (SessionId, Arc<str>);

/// A picture's copy as the terminal holds it: its first row's client id, how many consecutive
/// rows it answered, its pixels, the cells it was made for, how many rows are sent and the frame
/// that last showed it.
struct Held {
    id: u32,
    answered: u16,
    size: (u32, u32),
    cells: (u16, u16),
    sent: u16,
    seen: u64,
}

impl Held {
    // A copy refused by the read or terminal keeps its half blocks at any cells.
    fn refused(&self) -> bool {
        self.size == (0, 0)
    }

    // The pixels the terminal keeps for it.
    fn bytes(&self) -> u64 {
        u64::from(self.size.0) * u64::from(self.size.1) * 4
    }

    // The commands deleting its rows the terminal answered.
    fn delete(&self) -> String {
        (0..self.answered)
            .map(|r| delete(self.id + u32::from(r)))
            .collect()
    }
}

/// A row of a picture placed at `at`, its first `width` pixels over `columns` cells.
#[derive(PartialEq)]
struct Placement {
    id: u32,
    pid: usize,
    at: (u16, u16),
    width: u32,
    columns: u16,
}

impl Pictures {
    /// A multipart upload through the row encoder; only an answer naming its client id enables
    /// pictures. Delete it even without an answer. IDs start randomly above common small ids and
    /// are never reused in this run; another program's retained ids can still collide by chance.
    pub(super) fn query(&mut self) -> String {
        let mut seed = [0u8; 4];
        if getrandom::fill(&mut seed).is_err() {
            return String::new();
        }
        self.last = (u32::from_be_bytes(seed) & 0x3fff_ffff) | 0x4000_0000;
        self.probe = Some(self.last);
        // The command contains only ASCII controls, keys and base64.
        let query =
            String::from_utf8(command(self.last, "f=32,s=769,v=1", &[0; 3076])).unwrap_or_default();
        query + &delete(self.last)
    }

    /// Takes a Kitty answer's text after `ESC _G` and before its terminator. Only the exact,
    /// nonzero client id counts; an anonymous or late answer cannot acknowledge another row.
    pub(super) fn answer(&mut self, text: &str) {
        let (keys, message) = text.split_once(';').unwrap_or((text, ""));
        let id = keys
            .split(',')
            .find_map(|kv| kv.strip_prefix("i=")?.parse::<u32>().ok());
        let Some(id) = id.filter(|&id| id != 0) else {
            return;
        };
        if self.probe == Some(id) {
            self.probe = None;
            self.kitty = Some(message == "OK");
        } else {
            self.answered(id, message == "OK");
        }
    }

    // Only the awaited row counts. IDs are never reused, so a timed-out row's late answer cannot
    // name a later row; its delete already followed its complete upload in the ordered writer.
    fn answered(&mut self, id: u32, ok: bool) {
        let Some((_, left)) = self.waiting.take_if(|(n, _)| *n == id) else {
            return;
        };
        let took = left.map_or(Duration::ZERO, |at| at.elapsed());
        self.slowest = self.slowest.max(took);
        match (self.owner(id), ok) {
            (Some(key), true) => _ = self.held.get_mut(&key).map(|h| h.answered += 1),
            (Some(key), false) => {
                self.deletes += &delete(id);
                self.fail(&key);
            }
            (None, _) => self.deletes += &delete(id),
        }
    }

    // The held picture whose next row to be answered is `id`.
    fn owner(&self, id: u32) -> Option<Key> {
        let next = |h: &Held| h.answered < h.sent && h.id + u32::from(h.answered) == id;
        self.held
            .iter()
            .find(|(_, h)| next(h))
            .map(|(k, _)| k.clone())
    }

    // Gives up on the row awaited: deletes it by its client id, never reused here, and ends
    // its picture.
    fn give_up(&mut self) {
        if let Some((id, left)) = self.waiting.take() {
            self.slowest = self
                .slowest
                .max(left.map_or(Duration::ZERO, |at| at.elapsed()));
            self.deletes += &delete(id);
            if let Some(key) = self.owner(id) {
                self.fail(&key);
            }
        }
    }

    // Ends a picture the terminal refused a row of or did not answer in time: deletes the rows it
    // holds and keeps its half blocks, refused as a copy refused is, so no cells read it again.
    fn fail(&mut self, key: &Key) {
        if let Some(old) = self.held.remove(key) {
            self.release(&old);
            let refused = Held {
                answered: 0,
                size: (0, 0),
                sent: 0,
                ..old
            };
            self.held.insert(key.clone(), refused);
        }
        if self.sending.as_ref().is_some_and(|(k, ..)| k == key) {
            self.sending = None;
        }
    }

    /// The terminal has taken everything written: the row awaited, if not yet, now.
    pub(super) fn taken(&mut self) {
        if let Some((_, left)) = &mut self.waiting {
            left.get_or_insert_with(Instant::now);
        }
    }

    /// When the row awaited is given up on, once the terminal has taken it: after four times the
    /// slowest answer yet, and at least a second.
    pub(super) fn due(&self) -> Option<Instant> {
        let (_, left) = self.waiting?;
        Some(left? + (self.slowest * 4).max(Duration::from_secs(1)))
    }

    /// The commands deleting every picture the terminal holds for this app, which then holds
    /// none: before the app leaves the alternate screen, or after the screen was cleared. A row
    /// on its way is deleted once answered, or given up on; copies refused stay refused.
    pub(super) fn forget(&mut self) -> String {
        self.placed.clear();
        self.sending = None;
        let deletes = self.held.values().map(Held::delete).collect::<String>();
        self.held.retain(|_, h| h.refused());
        std::mem::take(&mut self.deletes) + &deletes
    }

    /// The next unit to write when the terminal has taken everything before it: the deletes
    /// waiting and, once the terminal has answered the last row sent, the next row of the picture
    /// being sent, as one complete Kitty image. A row unanswered when due is given up on. A picture
    /// the last frame did not draw stops sending; drawn again, it is read again for the rest.
    pub(super) fn row(&mut self) -> Option<Vec<u8>> {
        self.taken();
        if self.due().is_some_and(|due| due <= Instant::now()) {
            self.give_up();
        }
        let mut out = std::mem::take(&mut self.deletes).into_bytes();
        let Some((key, copy)) = self.sending.as_mut().filter(|_| self.waiting.is_none()) else {
            return (!out.is_empty()).then_some(out);
        };
        let Some(held) = self.held.get_mut(key).filter(|h| h.seen >= self.frame) else {
            self.sending = None;
            return (!out.is_empty()).then_some(out);
        };
        let ((w, h), rows) = (copy.dimensions(), u32::from(held.cells.1));
        let (top, bottom) = (
            u32::from(held.sent) * h / rows,
            (u32::from(held.sent) + 1) * h / rows,
        );
        let pixels = &copy.as_raw()[(top * w * 4) as usize..(bottom * w * 4) as usize];
        let (id, keys) = (
            held.id + u32::from(held.sent),
            format!("f=32,s={w},v={}", bottom - top),
        );
        out.extend(command(id, &keys, pixels));
        self.waiting = Some((id, None));
        held.sent += 1;
        if held.sent == held.cells.1 {
            self.sending = None;
        }
        Some(out)
    }

    // Holds `copy` as `key`'s, drawn in `cells`, and starts sending it: after the rows it holds
    // of the same copy, or in place of what it held, after deleting the pictures shown longest
    // ago past 64 or HELD, and not at all when the pictures shown leave no room. Without a copy
    // it holds none, sending nothing.
    fn hold(&mut self, key: Key, cells: (u16, u16), copy: Option<RgbaImage>) {
        // A refusal while this copy was being read still wins.
        if self.held.get(&key).is_some_and(Held::refused) {
            return;
        }
        let size = copy.as_ref().map_or((0, 0), RgbaImage::dimensions);
        // Reserve every row before sending any; exhaustion keeps half blocks instead of reusing
        // an id that a late answer could name.
        let Some(last) = self.last.checked_add(u32::from(cells.1)) else {
            self.kitty = Some(false);
            return;
        };
        let (id, seen) = (self.last + 1, self.frame);
        let mut held = Held {
            id,
            answered: 0,
            size,
            cells,
            sent: 0,
            seen,
        };
        match self.held.remove(&key) {
            Some(old) if (old.cells, old.size) == (cells, size) && copy.is_some() => {
                (held.id, held.answered, held.sent) = (old.id, old.answered, old.sent)
            }
            old => {
                self.last = last;
                if let Some(old) = old {
                    self.release(&old);
                }
            }
        }
        self.held.insert(key.clone(), held);
        let over =
            |p: &Self| p.held.len() > 64 || p.held.values().map(Held::bytes).sum::<u64>() > HELD;
        while over(self) {
            let old = (self.held.iter())
                .filter(|(k, h)| **k != key && h.seen < self.frame)
                .min_by_key(|(_, h)| h.seen)
                .map(|(k, _)| k.clone());
            let Some(old) = old.and_then(|old| self.held.remove(&old)) else {
                if let Some(held) = self.held.remove(&key) {
                    self.release(&held);
                }
                return;
            };
            self.release(&old);
        }
        self.sending = copy.map(|copy| (key, copy));
    }

    // Deletes a picture no longer held, and forgets its placements.
    fn release(&mut self, old: &Held) {
        self.deletes += &old.delete();
        self.placed
            .retain(|p| p.id < old.id || p.id - old.id >= u32::from(old.answered));
    }
}

/// A cell's pixels from the terminal's size report, else 8 by 16, a cell twice as tall as wide.
pub(super) fn cell() -> (u16, u16) {
    match crossterm::terminal::window_size() {
        Ok(s) if s.width > 0 && s.height > 0 && s.columns > 0 && s.rows > 0 => {
            ((s.width / s.columns).max(1), (s.height / s.rows).max(1))
        }
        _ => (8, 16),
    }
}

/// After a frame, starts reading the first picture it showed or asked for and lacks: a picture
/// never read, then, under Kitty, one drawn that the terminal holds for no cells or for others,
/// or holds only some rows of, unless its copy was refused; a copy only once none is being sent.
/// One read at a time.
pub(super) fn read_visible(app: &mut App) {
    if app.pictures.loading || app.pending >= 8 {
        return;
    }
    // A read starts only with room for its thumbnail, and the terminal takes a picture only with
    // room for the largest beside those this frame shows; until then the chip or the half blocks
    // stay, and nothing shown is dropped to make room.
    let unread = |t: &Transcript| t.shown() && t.unread.iter().any(|n| !t.pictures.contains_key(n));
    let thumbs = app.transcripts.values().any(unread) && trim(app);
    let (frame, held) = (app.pictures.frame, &app.pictures.held);
    let shown = held.values().filter(|h| h.seen >= frame);
    let room =
        shown.clone().count() < 64 && shown.map(Held::bytes).sum::<u64>() + 4 * PIXELS <= HELD;
    let copies = app.pictures.kitty == Some(true) && room && app.pictures.sending.is_none();
    let mut next = None;
    for (id, t) in app.transcripts.iter_mut().filter(|(_, t)| t.shown()) {
        if let Some(name) = t
            .unread
            .iter()
            .find(|n| thumbs && !t.pictures.contains_key(*n))
        {
            t.pictures.insert(name.clone(), (frame, None));
            next = Some((id.clone(), name.clone(), None));
            break;
        }
        let held = |f: &Fragment| held.get(&(id.clone(), f.name.clone()));
        // Lacking, too, the rest of a copy whose sending stopped; never a copy refused, by the
        // read or by the terminal, which keeps its half blocks in any cells.
        let again = |h: &Held, f: &Fragment| h.cells != f.cells || h.sent < h.cells.1;
        let lacks = |f: &&Fragment| copies && held(f).is_none_or(|h| !h.refused() && again(h, f));
        if let Some(f) = t.drawn.iter().find(lacks) {
            next = Some((id.clone(), f.name.clone(), Some(f.cells)));
            break;
        }
    }
    let Some((session, name, cells)) = next else {
        return;
    };
    let (cw, ch) = app.pictures.cell;
    let copy = cells.map(|(c, r)| (u32::from(c) * u32::from(cw), u32::from(r) * u32::from(ch)));
    app.pictures.loading = app.spawn(|api| async move {
        let loaded = load(&api, &session, &name, copy).await;
        move |app: &mut App| apply(app, (session, name), cells, loaded)
    });
}

// Reads the file whole in ranges, refusing it past ENCODED or changed during the read, then
// decodes it on a blocking thread.
async fn load(
    api: &Api,
    session: &SessionId,
    name: &str,
    copy: Option<(u32, u32)>,
) -> Option<((u32, u32), RgbaImage)> {
    let (mut bytes, mut size) = (Vec::new(), None);
    while size.is_none_or(|n| (bytes.len() as u64) < n) {
        let (part, total) = api
            .chunk(session, name, bytes.len() as u64, RANGE)
            .await
            .ok()?;
        if total > ENCODED || part.is_empty() || size.is_some_and(|n| n != total) {
            return None;
        }
        if size.replace(total).is_none() {
            bytes.reserve_exact(total as usize);
        }
        bytes.extend_from_slice(&part);
    }
    let decoded = tokio::task::spawn_blocking(move || decode(bytes, copy));
    decoded.await.ok()?
}

// Keeps a read's thumbnail, or its copy for `cells`; a refused or failed thumbnail keeps the
// chip, and a refused or failed copy the half blocks while the copy's refusal is held.
fn apply(
    app: &mut App,
    key: Key,
    cells: Option<(u16, u16)>,
    loaded: Option<((u32, u32), RgbaImage)>,
) {
    app.pictures.loading = false;
    let frame = app.pictures.frame;
    let Some(t) = app.transcripts.get_mut(&key.0) else {
        return;
    };
    t.invalidate();
    match (loaded, cells) {
        (None, None) => _ = t.pictures.insert(key.1, (frame, None)),
        (Some((size, thumb)), None) => _ = t.pictures.insert(key.1, (frame, Some((size, thumb)))),
        (copy, Some(cells)) => app.pictures.hold(key, cells, copy.map(|(_, copy)| copy)),
    }
}

// Makes room for one more thumbnail within THUMBS and one more name within 256 across every
// transcript, dropping those shown longest ago first but none this frame showed; false when that
// leaves no room.
fn trim(app: &mut App) -> bool {
    let frame = app.pictures.frame;
    let size = |p: &Option<Picture>| p.as_ref().map_or(0, |(_, thumb)| thumb.as_raw().len());
    let mut all: Vec<_> = (app.transcripts.iter())
        .flat_map(|(id, t)| {
            let known = t.pictures.iter();
            known.map(move |(name, (seen, p))| (*seen, id.clone(), name.clone(), size(p)))
        })
        .collect();
    all.sort_by_key(|(seen, ..)| *seen);
    let (mut bytes, mut names) = (all.iter().map(|a| a.3).sum::<usize>(), all.len());
    let thumb = (THUMB * THUMB * 4) as usize;
    for (seen, id, name, n) in all {
        if seen >= frame || bytes + thumb <= THUMBS && names < 256 {
            break;
        }
        if let Some(t) = app.transcripts.get_mut(&id) {
            t.pictures.remove(&name);
            t.invalidate();
        }
        (bytes, names) = (bytes - n, names - 1);
    }
    bytes + thumb <= THUMBS && names < 256
}

// The bytes of Kitty's command for `len` bytes of data: base64 and each piece's keys.
fn encoding(len: usize) -> usize {
    len.div_ceil(3) * 4 + len.div_ceil(3072) * 24 + 64
}

// Kitty's command sending `data` with client `id` in the format `keys` names, in pieces of
// 4,096 base64 characters. Repeat the id only on the final continuation: iTerm replies using
// that command's fields, while Kitty and Ghostty use the first chunk's saved fields.
fn command(id: u32, keys: &str, data: &[u8]) -> Vec<u8> {
    let pieces = data.len().div_ceil(3072);
    let mut out = Vec::with_capacity(encoding(data.len()) + keys.len());
    for (i, piece) in data.chunks(3072).enumerate() {
        let first = match i {
            0 => format!("a=t,{keys},i={id},"),
            _ if i + 1 == pieces => format!("i={id},"),
            _ => String::new(),
        };
        let more = u8::from(i + 1 < pieces);
        out.extend_from_slice(format!("\x1b_G{first}m={more};").as_bytes());
        let start = out.len();
        out.resize(start + piece.len().div_ceil(3) * 4, 0);
        // The slice is the encoded length exactly.
        let _ = STANDARD.encode_slice(piece, &mut out[start..]);
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

fn delete(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
}

/// The frame's Kitty commands, written after its cells inside its synchronized update: the
/// deletes waiting, then each row the terminal holds of a picture placed over its ground, clipped
/// to the cells nothing drawn later covered, above text (iTerm2 draws the default z-index 0 under
/// text); a moved or clipped placement replaced, a gone one deleted, and nothing for an unchanged
/// frame. Call before the cell diff: each placement replaces its fallback cells with their ground,
/// and a row not answered keeps its fallback.
pub(super) fn place(app: &mut App, b: &mut Buffer) -> Vec<u8> {
    let p = &mut app.pictures;
    let mut out = std::mem::take(&mut p.deletes);
    let mut now: Vec<Placement> = Vec::new();
    for (session, t) in &app.transcripts {
        for f in &t.drawn {
            let key = (session.clone(), f.name.clone());
            let Some(held) = p.held.get_mut(&key) else {
                continue;
            };
            if held.cells != f.cells && !held.refused() {
                if p.sending.as_ref().is_some_and(|(k, _)| *k == key) {
                    p.sending = None;
                }
                continue;
            }
            held.seen = p.frame;
            let (columns, rows) = intact(b, f);
            let rows = rows.min(held.answered.saturating_sub(f.row));
            if columns == 0 || rows == 0 {
                continue;
            }
            // Keep the fallback until ownership has been checked against the complete frame;
            // blank cells could otherwise match a later overlay's and put a picture above it.
            for y in f.at.1..f.at.1 + rows {
                for x in f.at.0..f.at.0 + columns {
                    if let Some(cell) = b.cell_mut((x, y)) {
                        cell.set_char(' ').set_fg(f.ground).set_bg(f.ground);
                    }
                }
            }
            let width = (u32::from(columns) * held.size.0 / u32::from(f.cells.0)).max(1);
            for row in 0..rows {
                let id = held.id + u32::from(f.row + row);
                let pid = 1 + now.iter().filter(|n| n.id == id).count();
                let at = (f.at.0, f.at.1 + row);
                now.push(Placement {
                    id,
                    pid,
                    at,
                    width,
                    columns,
                });
            }
        }
    }
    for old in (p.placed.iter()).filter(|o| !now.iter().any(|n| (n.id, n.pid) == (o.id, o.pid))) {
        out += &format!("\x1b_Ga=d,d=i,i={},p={},q=2\x1b\\", old.id, old.pid);
    }
    for n in now.iter().filter(|n| !p.placed.contains(n)) {
        let ((x, y), (id, pid, w, c)) = (n.at, (n.id, n.pid, n.width, n.columns));
        out += &format!(
            "\x1b[{};{}H\x1b_Ga=p,i={id},p={pid},w={w},c={c},r=1,z=1,C=1,q=2\x1b\\",
            y + 1,
            x + 1,
        );
    }
    p.placed = now;
    match out.is_empty() {
        true => Vec::new(),
        false => format!("\x1b7{out}\x1b8").into_bytes(),
    }
}

// The largest rectangle from a fragment's first cell whose every cell is still the one the
// transcript drew there: its columns and rows.
fn intact(b: &Buffer, f: &Fragment) -> (u16, u16) {
    let (mut best, mut width) = ((0, 0), f.width);
    for row in 0..f.rows {
        let drawn = &f.drawn[usize::from(row) * usize::from(f.width)..];
        let same = |&x: &u16| b.cell((f.at.0 + x, f.at.1 + row)) == drawn.get(usize::from(x));
        width = (0..width).take_while(same).count() as u16;
        if u32::from(width) * u32::from(row + 1) > u32::from(best.0) * u32::from(best.1) {
            best = (width, row + 1);
        }
    }
    best
}
