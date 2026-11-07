//! What a file's first bytes say it is: an image's or a video's kind, and an image's size.

/// A PNG, JPEG or WebP image's media type and file extension, from its first bytes; `None` for
/// any other bytes, whatever the file is named.
pub(crate) fn image_type(head: &[u8]) -> Option<(&'static str, &'static str)> {
    match head {
        [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', ..] => Some(("image/png", "png")),
        [0xff, 0xd8, 0xff, ..] => Some(("image/jpeg", "jpg")),
        _ if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") => {
            Some(("image/webp", "webp"))
        }
        _ => None,
    }
}

/// `video/mp4` or `video/quicktime` from a file's first bytes, an ISO media `ftyp` box (brand
/// `qt  ` for QuickTime); `None` for any other bytes.
pub(crate) fn video_type(head: &[u8]) -> Option<&'static str> {
    match (head.get(4..8), head.get(8..12)) {
        (Some(b"ftyp"), Some(b"qt  ")) => Some("video/quicktime"),
        (Some(b"ftyp"), Some(_)) => Some("video/mp4"),
        _ => None,
    }
}

/// Dimensions from the first bytes and total length of a PNG, JPEG, WebP or GIF.
/// Returns none when the bounded header is incomplete or malformed.
pub(crate) fn image_pixels(b: &[u8], total: usize) -> Option<(u32, u32)> {
    let int = |at: usize, n: usize, big: bool| {
        let bytes = b.get(at..at + n)?;
        let fold = |v: u32, x: &u8| v << 8 | u32::from(*x);
        Some(match big {
            true => bytes.iter().fold(0, fold),
            false => bytes.iter().rev().fold(0, fold),
        })
    };
    // A WebP chunk at least `least` bytes long and inside the file.
    let chunk =
        |least: u32| int(16, 4, false).is_some_and(|n| n >= least && 20 + n as usize <= total);
    let found = match b.get(..4).zip(b.get(12..16)) {
        Some((b"\x89PNG", b"IHDR")) if int(8, 4, true) == Some(13) => {
            (int(16, 4, true)?, int(20, 4, true)?)
        }
        Some((b"RIFF", b"VP8X")) if chunk(10) => (int(24, 3, false)? + 1, int(27, 3, false)? + 1),
        Some((b"RIFF", b"VP8 ")) if chunk(10) && b.get(23..26) == Some(b"\x9d\x01\x2a") => {
            (int(26, 2, false)? & 0x3fff, int(28, 2, false)? & 0x3fff)
        }
        Some((b"RIFF", b"VP8L")) if chunk(5) && b.get(20) == Some(&0x2f) => {
            let v = int(21, 4, false)?;
            ((v & 0x3fff) + 1, (v >> 14 & 0x3fff) + 1)
        }
        _ if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") => {
            (int(6, 2, false)?, int(8, 2, false)?)
        }
        // A JPEG's segments up to its frame header, which holds height then width.
        _ if b.starts_with(b"\xff\xd8") => {
            let mut at = 2;
            loop {
                let kind = *b.get(at + 1).filter(|_| b.get(at) == Some(&0xff))?;
                let len =
                    int(at + 2, 2, true).filter(|&n| n >= 2 && at + 2 + n as usize <= total)?;
                if (0xc0..=0xcf).contains(&kind) && ![0xc4, 0xc8, 0xcc].contains(&kind) {
                    break (
                        int(at + 7, 2, true).filter(|_| len >= 8)?,
                        int(at + 5, 2, true)?,
                    );
                }
                at += 2 + len as usize;
            }
        }
        _ => return None,
    };
    Some(found).filter(|&(w, h)| w > 0 && h > 0)
}
