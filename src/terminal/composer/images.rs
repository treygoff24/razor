//! Images: a pasted path is read off the loop, sniffed and attached as a numbered `[Image #N]`
//! chip with a removable tray row; only chips still in the words are sent.
use super::super::{
    App, Error, error, fail,
    text::*,
    view::{Action, Hit},
};
use super::Composer;
use crate::protocol::{
    BODY_BYTES, IMAGE_BASE64, IMAGES_PER_INPUT, ImageCopy, ImageOrigin, SessionId,
};
use ratatui::{buffer::Buffer, layout::Rect};

pub struct Loaded {
    pub number: usize,
    data: String,
    origin: Option<(String, bool)>,
    /// Width and height, when the header in the picture's first 256 KiB names them.
    pixels: Option<(u32, u32)>,
}

// Header parsing is shared with the request's image preview.
fn pixels(data: &str) -> Option<(u32, u32)> {
    use base64::Engine;
    let head = &data[..data.len().min(1 << 18) / 4 * 4];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(head)
        .ok()?;
    let total = data.len() / 4 * 3 - data.bytes().rev().take_while(|&c| c == b'=').count();
    crate::protocol::image_pixels(&bytes, total)
}

fn marker(number: usize) -> String {
    format!("[Image #{number}]")
}

impl Composer {
    /// The attachments whose chips are still in the words.
    pub fn active_images(&self) -> impl DoubleEndedIterator<Item = &Loaded> {
        self.images
            .iter()
            .filter(|image| self.text.contains(&marker(image.number)))
    }

    /// The chip that ends just before the cursor, or with `after` starts just after it, when
    /// nothing is selected: Backspace or Delete there removes it whole.
    pub fn image_by_cursor(&self, after: bool) -> Option<usize> {
        let (before, rest) = self.text.split_at(self.cursor);
        let at = |m: &str| {
            if after {
                rest.starts_with(m)
            } else {
                before.ends_with(m)
            }
        };
        let chip = self.images.iter().find(|image| at(&marker(image.number)));
        chip.filter(|_| self.selection().is_none())
            .map(|image| image.number)
    }

    /// The launch's images, each with its chip after the words.
    pub fn initial_images(&mut self, images: Vec<String>) {
        for data in images {
            self.next_image += 1;
            let number = self.next_image;
            self.replace(format!("{} {}", self.text, marker(number)));
            self.images.push(Loaded {
                pixels: pixels(&data),
                number,
                data,
                origin: None,
            });
        }
    }

    /// Refuses an input past the request limit, or with a chip whose image is gone.
    pub fn validate_images(&self) -> Result<(), Error> {
        // Before base64 is cloned into JSON; the client still checks its exact encoded size.
        let bytes: usize = self.active_images().map(|image| image.data.len()).sum();
        if bytes.saturating_add(self.text.len()) > BODY_BYTES {
            return Err(error(
                "The input exceeds the server's 32 MiB request limit; remove an image or shorten the words.",
            ));
        }
        for (at, _) in self.text.match_indices("[Image #") {
            let number = self.text[at + 8..]
                .split_once(']')
                .and_then(|(n, _)| n.parse::<usize>().ok());
            if number.is_some_and(|n| !self.images.iter().any(|image| image.number == n)) {
                return Err(error(
                    "An image marker has no attachment; paste its image path again or remove the marker before sending.",
                ));
            }
        }
        Ok(())
    }

    pub fn images_for_send(&self) -> Vec<String> {
        self.active_images()
            .filter(|image| !image.data.is_empty())
            .map(|image| image.data.clone())
            .collect()
    }

    /// Keep a local image's origin with the input even when only its pointer could be sent.
    pub fn image_origins(&self) -> Vec<ImageOrigin> {
        let machine = crate::profiles::machine();
        self.active_images()
            .filter_map(|image| {
                let (path, fitted) = image.origin.as_ref()?;
                let copy = if image.data.is_empty() {
                    ImageCopy::PointerOnly
                } else if *fitted {
                    ImageCopy::Scaled
                } else {
                    return None;
                };
                Some(ImageOrigin {
                    number: image.number,
                    path: path.clone(),
                    device: machine.clone(),
                    copy,
                })
            })
            .collect()
    }

    /// Removes a chip from the words, keeping the image for undo; false when it is not there.
    pub fn remove_image(&mut self, number: usize) -> bool {
        let marker = marker(number);
        if !self.images.iter().any(|image| image.number == number) || !self.text.contains(&marker) {
            return false;
        }
        let before = self
            .text
            .match_indices(&marker)
            .filter(|(at, _)| *at < self.cursor);
        let removed: usize = before
            .map(|(at, _)| (self.cursor - at).min(marker.len()))
            .sum();
        let cursor = self.cursor.saturating_sub(removed);
        self.save();
        self.replace(self.text.replace(&marker, ""));
        self.cursor = cursor;
        true
    }

    // Replaces the pasted path with its chip; at twenty images the oldest removed one goes.
    // The path's last occurrence standing alone between blanks, else its last at all.
    fn attach(&mut self, data: String, origin: String, fitted: bool, path: &str) -> bool {
        let edge = |c: Option<char>| c.is_none_or(char::is_whitespace);
        let alone = |&at: &usize| {
            edge(self.text[..at].chars().next_back())
                && edge(self.text[at + path.len()..].chars().next())
        };
        let mut found = self.text.rmatch_indices(path).map(|(at, _)| at);
        let Some(at) = found.clone().find(alone).or(found.next()) else {
            return false;
        };
        if self.images.len() == IMAGES_PER_INPUT {
            let Some(old) = self
                .images
                .iter()
                .position(|image| !self.text.contains(&marker(image.number)))
            else {
                return false;
            };
            self.images.remove(old);
            // Retiring an image's bytes retires the undo texts that could name it.
            (self.undo, self.redo, self.undo_bytes) = Default::default();
        }
        self.next_image += 1;
        let number = self.next_image;
        let mut text = std::mem::take(&mut self.text);
        text.replace_range(at..at + path.len(), &marker(number));
        self.replace(text);
        self.images.push(Loaded {
            pixels: pixels(&data),
            number,
            data,
            origin: Some((origin, fitted)),
        });
        true
    }
}

// A pasted image's file: one path, quoted, backslash-escaped as a drag from Finder pastes it,
// a `file://` address (percent-decoded, on this machine or `localhost`) or under `~/`.
fn image_path(text: &str) -> Option<String> {
    let text = text.trim().trim_matches('"').trim_matches('\'');
    let url = reqwest::Url::parse(text).ok();
    let mut path = match url.filter(|u| u.scheme() == "file") {
        Some(url) => url.to_file_path().ok()?.to_str()?.to_owned(),
        None => {
            let (mut chars, mut path) = (text.chars(), String::new());
            while let Some(c) = chars.next() {
                path.push(if c == '\\' { chars.next()? } else { c });
            }
            path
        }
    };
    if let Some(rest) = path.strip_prefix("~/") {
        path = format!("{}/{rest}", std::env::var("HOME").ok()?);
    }
    let lower = path.to_lowercase();
    let ext = [".png", ".jpg", ".jpeg", ".webp"]
        .iter()
        .any(|e| lower.ends_with(e));
    (path.starts_with('/') && ext).then_some(path)
}

/// A paste: words go in at the cursor; each image path in it also starts its read off the loop.
/// A path is a line, or a line's piece from a space before `/` to the next.
pub fn paste(app: &mut App, text: String) {
    if text.is_empty() {
        return app.say("The paste was empty: if it was a picture, this terminal sent no image, so paste its file's path.");
    }
    let (mut files, mut heic) = (Vec::new(), None);
    for line in text.lines() {
        let cuts = || line.match_indices(" /").map(|(at, _)| at);
        let pieces =
            (std::iter::once(0).chain(cuts().map(|at| at + 1))).zip(cuts().chain([line.len()]));
        for piece in pieces.map(|(start, end)| line[start..end].trim()) {
            files.extend(image_path(piece).map(|file| (file, piece.to_owned())));
            let lower = piece.to_lowercase();
            if lower.ends_with(".heic") || lower.ends_with(".heif") {
                heic = std::path::Path::new(piece)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
            }
        }
    }
    let image = !files.is_empty();
    if image && app.sending.is_some() {
        return app.say("Input admission is pending; paste the image once it answers.");
    }
    if !image && !app.composer.insert(&text) {
        app.say("The paste exceeds 1 MiB; send a file path instead.");
    } else if let Some(name) = heic.filter(|_| !image) {
        app.say(&format!(
            "{name} is HEIC, which cannot be sent; only its name reaches the model."
        ));
    } else if !image {
        let chars = unicode_segmentation::UnicodeSegmentation::graphemes(&*text, true).count();
        let lines = text.lines().count();
        let s = plural(lines);
        app.say(&format!(
            "Pasted {} chars · {lines} line{s}",
            thousands(chars)
        ));
    } else if image && (app.pending >= 8 || !app.composer.insert(&text)) {
        app.say(
            "The paste could not be attached; wait for pending operations and try its path again.",
        );
    } else {
        // A refused read leaves its path as words and the rest unread; only reads begun count.
        for (path, piece) in files {
            let target = app.focus.clone();
            let read = app.spawn(|_| async move {
                let loaded = load(path.clone())
                    .await
                    .map(|(data, fitted)| (data, path, fitted));
                move |app: &mut App| loaded_image(app, loaded, piece, target)
            });
            if !read {
                break;
            }
            app.loading_images += 1;
        }
    }
    app.edited();
}

fn loaded_image(
    app: &mut App,
    loaded: Result<(String, String, bool), Error>,
    path: String,
    target: Option<SessionId>,
) {
    app.loading_images = app.loading_images.saturating_sub(1);
    match loaded {
        Err(e) => app.say(&e.to_string()),
        Ok((data, origin, fitted)) if app.focus == target => {
            if app.composer.attach(data, origin, fitted, &path) {
                app.edited();
            } else if app.composer.active_images().count() == IMAGES_PER_INPUT {
                app.say(
                    "An input allows 20 images; remove one or send these before attaching another.",
                );
            }
        }
        Ok(_) => {}
    }
}

// Read bounded input for fitting; a file outside the bound stays on its device as a pointer.
async fn load(path: String) -> Result<(String, bool), Error> {
    use base64::Engine;
    use tokio::io::AsyncReadExt;
    static READ: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let _permit = READ
        .acquire()
        .await
        .map_err(|_| error("Image reader unavailable."))?;
    let unreadable =
        "The pasted image path cannot be read; paste an existing JPEG, PNG or WebP path.";
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(fail(unreadable))?;
    let mut bytes = Vec::new();
    file.take((32 << 20) + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(fail(unreadable))?;
    if crate::protocol::image_type(&bytes).is_none() {
        return Err(error(
            "The pasted file is not JPEG, PNG or WebP; choose a supported image.",
        ));
    }
    if bytes.len() > 32 << 20 {
        return Ok((String::new(), false));
    }
    let dimensions = crate::protocol::image_pixels(&bytes, bytes.len());
    let needs_fit = bytes.len().div_ceil(3) * 4 > IMAGE_BASE64
        || dimensions.is_none_or(|(w, h)| w > 3000 || h > 3000);
    if !needs_fit {
        return Ok((
            base64::engine::general_purpose::STANDARD.encode(bytes),
            false,
        ));
    }
    let original_fits = bytes.len().div_ceil(3) * 4 <= IMAGE_BASE64;
    let fitted = tokio::task::spawn_blocking(move || {
        let original = original_fits.then(|| bytes.clone());
        crate::terminal::image_codec::fitted(bytes)
            .map(|bytes| (bytes, true))
            .or_else(|| original.map(|bytes| (bytes, false)))
    })
    .await
    .ok()
    .flatten();
    Ok(match fitted {
        Some((bytes, fitted)) => (
            base64::engine::general_purpose::STANDARD.encode(bytes),
            fitted,
        ),
        None => (String::new(), false),
    })
}

/// The tray's one row while images wait.
pub fn image_rows(composer: &Composer) -> u16 {
    u16::from(composer.active_images().next().is_some())
}

/// The tray: one row of chips above the input, `[Image #1] 640×480 · 42 KiB ×` when all fit with their
/// sizes, else bare labels from a window that keeps the chosen chip in view, then `+N more`. A
/// click on a chip chooses it, on its `×` removes it; with one chosen, the keys' hint follows.
pub fn draw_images(composer: &Composer, b: &mut Buffer, r: Rect, hits: &mut Vec<Hit>) {
    let images: Vec<_> = composer.active_images().collect();
    if images.is_empty() || r.height == 0 {
        return;
    }
    fill(b, Rect::new(r.x, r.y, r.width, 1), style(DATA, RECESS));
    let chosen = composer.chosen.filter(|&at| at < images.len());
    let size = |im: &Loaded| {
        if im.data.is_empty() {
            return String::new();
        }
        let pixels = im
            .pixels
            .map_or(String::new(), |(w, h)| format!(" {w}×{h} ·"));
        match im.data.len() * 3 / 4 {
            n @ ..1_048_576 => format!("{pixels} {} KiB", n.div_ceil(1024)),
            n => format!("{pixels} {:.2} MiB", n as f64 / 1_048_576.),
        }
    };
    let label = |im: &Loaded| {
        let status = if im.data.is_empty() {
            " pointer"
        } else if im.origin.as_ref().is_some_and(|(_, fitted)| *fitted) {
            " scaled"
        } else {
            ""
        };
        format!("{}{status}", marker(im.number))
    };
    let cell =
        |im: &Loaded, full: bool| width(&label(im)) + 5 + if full { width(&size(im)) } else { 0 };
    let room = usize::from(r.width.saturating_sub(1));
    let gaps = images.len() - 1;
    let full = images.iter().map(|im| cell(im, true)).sum::<usize>() + gaps <= room;
    let more = |n: usize| format!(" +{n} more");
    // How many chips from `start` fit with the count of the rest.
    let fits = |start: usize| {
        let (mut used, mut shown) = (0, 0);
        for im in &images[start..] {
            let rest = images.len() - shown - 1;
            let need = cell(im, full) + usize::from(shown > 0);
            if used + need + if rest > 0 { width(&more(rest)) } else { 0 } > room {
                break;
            }
            (used, shown) = (used + need, shown + 1);
        }
        shown
    };
    let mut start = 0;
    while chosen.is_some_and(|at| at >= start + fits(start)) && start + 1 < images.len() {
        start += 1;
    }
    let shown = fits(start);
    let mut x = r.x + 1;
    for (i, im) in images.iter().enumerate().skip(start).take(shown) {
        x += u16::from(i > start);
        let lit = chosen == Some(i);
        let look = match lit {
            true => style(GLASS, CYAN).add_modifier(ratatui::style::Modifier::BOLD),
            false => style(WHITE, CHIP),
        };
        let at = x;
        x = put(b, x, r.y, &format!(" {} ", label(im)), r.right() - x, look);
        hits.push((Rect::new(at, r.y, x - at, 1), Action::Chip(i)));
        if full {
            x = put(b, x, r.y, &size(im), r.right() - x, style(GRAY, RECESS));
        }
        let cross = style([GRAY, WHITE][usize::from(lit)], RECESS)
            .add_modifier(ratatui::style::Modifier::BOLD);
        let at = x;
        x = put(b, x, r.y, " × ", r.right() - x, cross);
        hits.push((
            Rect::new(at, r.y, x - at, 1),
            Action::RemoveImage(im.number),
        ));
    }
    let gray = style(GRAY, RECESS);
    if shown < images.len() {
        put(b, x, r.y, &more(images.len() - shown), r.right() - x, gray);
    }
}
