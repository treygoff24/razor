//! Bounded decoding and scaling for images displayed or sent to a model.
use image::{
    DynamicImage, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, ImageReader, Limits,
    RgbaImage,
};
use std::{
    io::{self, Cursor, Write},
    sync::Mutex,
};

// Bound decoded pixels and decoder allocations; leave 8 MiB for allocator overhead.
pub(crate) const PIXELS: u64 = 8 << 20;
pub(crate) const THUMB: u32 = 256;
const SIDE: u32 = 16_384;
const ALLOC: u64 = 16 << 20;
const HEAP: u64 = 120 << 20;
const METADATA: usize = 1 << 20;
pub(crate) const SENT: u32 = 2048;
static DECODING: Mutex<()> = Mutex::new(());
// A decode's size in pixels and its pixels.
type Decoded = ((u32, u32), DynamicImage);

// Screens the file's structure, then decodes it once within the bounds above into a thumbnail,
// or with `copy` into Kitty's copy at those pixels (see `clamp`). None for anything refused or
// undecodable.
pub(crate) fn decode(bytes: Vec<u8>, copy: Option<(u32, u32)>) -> Option<((u32, u32), RgbaImage)> {
    let _guard = DECODING.lock().ok()?;
    let (size, image) = decoded(bytes, copy, false)?;
    let pixels = match copy {
        None => fit(&image, THUMB),
        Some(copy) => scale(&image, clamp(size, copy)),
    };
    Some((size, pixels))
}

// Screen and decode through the same bounded path for display and fitting.
fn decoded(bytes: Vec<u8>, copy: Option<(u32, u32)>, fitting: bool) -> Option<Decoded> {
    match crate::protocol::image_type(&bytes)?.1 {
        "png" => png(&bytes, copy, fitting),
        "jpg" => {
            let planes = jpeg(&bytes)?;
            pixels(bytes, ImageFormat::Jpeg, copy, fitting, planes)
        }
        "webp" if webp(&bytes) => pixels(bytes, ImageFormat::WebP, copy, fitting, false),
        _ => None,
    }
}

// Stop an encoding as soon as it cannot fit; neither the encoder nor its output grows
// with the original file, and its output is held only once.
struct Capped(Vec<u8>);
impl Write for Capped {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len() + bytes.len() > crate::protocol::IMAGE_BYTES {
            return Err(io::Error::other("image exceeds the provider limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Re-encode an oversized attachment within the shared heap bound. Refusal leaves the
/// original on its device and lets the caller send its pointer instead.
pub(crate) fn fitted(bytes: Vec<u8>) -> Option<Vec<u8>> {
    let _guard = DECODING.lock().ok()?;
    let jpeg = crate::protocol::image_type(&bytes)?.1 == "jpg";
    let (_, image) = decoded(bytes, Some((2576, 2576)), true)?;
    let alpha = image.color().has_alpha();
    let mut side = 2576;
    for _ in 0..10 {
        let scaled = fit(&image, side);
        let mut out = Capped(Vec::with_capacity(crate::protocol::IMAGE_BYTES));
        let encoded = (|| {
            if jpeg {
                let rgb = DynamicImage::ImageRgba8(scaled).into_rgb8();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
                    .encode(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .ok()?;
            } else {
                let (w, h) = scaled.dimensions();
                let (color, raw) = if alpha {
                    (png::ColorType::Rgba, scaled.into_raw())
                } else {
                    (
                        png::ColorType::Rgb,
                        DynamicImage::ImageRgba8(scaled).into_rgb8().into_raw(),
                    )
                };
                let mut encoder = png::Encoder::new(&mut out, w, h);
                encoder.set_color(color);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_compression(png::Compression::Fastest);
                let mut writer = encoder.write_header().ok()?;
                let mut stream = writer.stream_writer().ok()?;
                stream.write_all(&raw).ok()?;
                stream.finish().ok()?;
            }
            Some(())
        })();
        if encoded.is_some() {
            return Some(out.0);
        }
        side = (side * 9 / 10).max(1);
    }
    None
}

// JPEG or WebP pixels through the image crate: its limits set before the decoder reads anything,
// and the pixel count and heap checked from the header before the output is allocated.
fn pixels(
    bytes: Vec<u8>,
    format: ImageFormat,
    copy: Option<(u32, u32)>,
    fitting: bool,
    planes: bool,
) -> Option<Decoded> {
    let encoded = bytes.len() as u64;
    // The JPEG decoder copies the bytes and drops these; the WebP decoder reads them in place.
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    (limits.max_image_width, limits.max_image_height) = (Some(SIDE), Some(SIDE));
    limits.max_alloc = Some(ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    let fits = heap(format, encoded, (w, h), copy, planes) + retained(encoded, fitting) <= HEAP;
    (fits && (fitting || u64::from(w) * u64::from(h) <= PIXELS)).then_some(())?;
    let orientation = match format {
        ImageFormat::Jpeg => decoder.orientation().ok()?,
        _ => image::metadata::Orientation::NoTransforms,
    };
    // Both decode to one byte a channel, at most four channels. JPEG's decode estimate also
    // covers the second pixel buffer a quarter-turn needs, after the decoder has gone.
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    Some((image.dimensions(), image))
}

// Intake retains the original as a fallback when its base64 fits the provider limit.
fn retained(encoded: u64, fitting: bool) -> u64 {
    if fitting && encoded <= crate::protocol::IMAGE_BYTES as u64 {
        encoded
    } else {
        0
    }
}

// The most heap one read asks for at once, in bytes, from the decoders' source: while decoding,
// the output at four bytes a pixel, our copy of the `encoded` file, the decoder's copies of it
// and its buffers by width and by pixel; after, beside the output, the thumbnail or, with
// `copy`, Kitty's copy at those pixels (see `clamp`) scaled in the output's type and as RGBA.
fn heap(
    format: ImageFormat,
    encoded: u64,
    (w, h): (u32, u32),
    copy: Option<(u32, u32)>,
    planes: bool,
) -> u64 {
    let copy = copy.map(|copy| clamp((w, h), copy));
    let copy = copy.map_or(0, |(cw, ch)| u64::from(cw) * u64::from(ch));
    let (e, w, h) = (encoded, u64::from(w), u64::from(h));
    let decoding = match format {
        // Unfiltering and interlace rows of up to eight bytes a pixel and a filter byte, a row
        // of output, and the inflater's state with the ancillary chunks it keeps and copies.
        ImageFormat::Png => e + 20 * (8 * w + 1) + 4 * w + (3 << 20),
        // The decoder's copy, its header copies with metadata capped, its row buffers, and a
        // progressive or non-interleaved coefficients: two bytes per component, on
        // blocks padded to 32 pixels.
        ImageFormat::Jpeg => {
            2 * e + (2_400 << 10) + 1048 * w + u64::from(planes) * 8 * (w + 31) * (h + 31)
        }
        // The last partition read whole into twice its size and copied, a partition's declared
        // size up to 16 MiB, and the frame's planes and alpha on whole macroblocks.
        _ => 4 * e + (16 << 20) + 3 * (w + 16) * (h + 16),
    };
    (4 * w * h + decoding).max(4 * w * h + 8 * copy) + u64::from(THUMB * THUMB * 4)
}

// The image within `side` by `side`, its aspect kept, as RGBA; never enlarged.
fn fit(image: &DynamicImage, side: u32) -> RgbaImage {
    match image.width().max(image.height()) > side {
        true => image.thumbnail(side, side).into_rgba8(),
        false => image.to_rgba8(),
    }
}

// Bound a requested copy by the greater of the original pixel count and SENT squared.
// For display this covers cell-size changes; fitting uses it as a conservative estimate.
fn clamp((w, h): (u32, u32), (cw, ch): (u32, u32)) -> (u32, u32) {
    let most = (u64::from(w) * u64::from(h)).max(u64::from(SENT * SENT)) as f64;
    let k = (most / (f64::from(cw) * f64::from(ch))).sqrt().min(1.);
    let side = |n: u32| ((f64::from(n) * k) as u32).max(1);
    (side(cw), side(ch))
}

// The image as RGBA at `w` by `h` pixels: averaged down, or enlarged each pixel the nearest
// source pixel, whole blocks where `w` and `h` are whole multiples of its size.
fn scale(image: &DynamicImage, (w, h): (u32, u32)) -> RgbaImage {
    let (sw, sh) = image.dimensions();
    match w > sw || h > sh {
        true => RgbaImage::from_fn(w, h, |x, y| image.get_pixel(x * sw / w, y * sh / h)),
        false => image.thumbnail_exact(w, h).into_rgba8(),
    }
}

// A PNG's size and pixels through the png crate: its chunks screened first, text and
// colour-profile chunks skipped unread, the pixel count and heap checked from its header before
// any pixel buffer, and the output stripped to a byte a channel.
fn png(bytes: &[u8], copy: Option<(u32, u32)>, fitting: bool) -> Option<Decoded> {
    let mut at = 8;
    let mut ancillary = 0;
    loop {
        let head = bytes.get(at..at + 8)?;
        let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let named = head[4..8].iter().all(u8::is_ascii_alphabetic);
        if !named || size > i32::MAX as usize || at + 12 + size > bytes.len() {
            return None;
        }
        if &head[4..8] == b"IEND" {
            break;
        }
        // A lower-case first letter marks an ancillary chunk.
        ancillary += size * usize::from(head[4].is_ascii_lowercase());
        if ancillary > METADATA {
            return None;
        }
        at += 12 + size;
    }
    let limits = png::Limits {
        bytes: ALLOC as usize,
    };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(bytes), limits);
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let header = decoder.read_header_info().ok()?;
    let (w, h) = (header.width, header.height);
    let encoded = bytes.len() as u64;
    let pixels = u64::from(w) * u64::from(h);
    if w > SIDE || h > SIDE || (!fitting && pixels > PIXELS) {
        return None;
    }
    (heap(ImageFormat::Png, encoded, (w, h), copy, false) + retained(encoded, fitting) <= HEAP)
        .then_some(())?;
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(frame.buffer_size());
    let (fw, fh) = (frame.width, frame.height);
    let image = match frame.color_type {
        png::ColorType::Grayscale => {
            DynamicImage::ImageLuma8(ImageBuffer::from_raw(fw, fh, buffer)?)
        }
        png::ColorType::GrayscaleAlpha => {
            DynamicImage::ImageLumaA8(ImageBuffer::from_raw(fw, fh, buffer)?)
        }
        png::ColorType::Rgb => DynamicImage::ImageRgb8(ImageBuffer::from_raw(fw, fh, buffer)?),
        png::ColorType::Rgba => DynamicImage::ImageRgba8(ImageBuffer::from_raw(fw, fh, buffer)?),
        png::ColorType::Indexed => return None,
    };
    Some(((w, h), image))
}

// A JPEG's markers from its start to its end, each segment whole within the file and every
// scan's entropy-coded data skipped by its stuffing rules, with at most 64 metadata segments
// (application data and comments) holding at most METADATA between them.
fn jpeg(bytes: &[u8]) -> Option<bool> {
    let (mut at, mut count, mut size) = (2, 0, 0);
    let (mut components, mut planes, mut scanned) = (None, false, false);
    loop {
        // Fill bytes may precede a marker.
        while bytes.get(at..at + 2) == Some(&[0xff, 0xff]) {
            at += 1;
        }
        let (Some(0xff), Some(&kind)) = (bytes.get(at), bytes.get(at + 1)) else {
            return None;
        };
        at += 2;
        // Restarts belong inside a scan's data, where the loop below takes them.
        match kind {
            0xd9 => return Some(planes || !scanned),
            0x00 | 0x01 | 0xd0..=0xd8 => return None,
            _ => {}
        }
        let Some(&[high, low]) = bytes.get(at..at + 2) else {
            return None;
        };
        let length = usize::from(u16::from_be_bytes([high, low]));
        if length < 2 || at + length > bytes.len() {
            return None;
        }
        // Baseline non-interleaved scans also allocate full coefficient planes in zune-jpeg.
        if matches!(kind, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            planes |= kind != 0xc0 || components.is_some();
            components = bytes.get(at + 7).copied();
        }
        if kind == 0xda && !scanned {
            planes |= components.is_none() || bytes.get(at + 2).copied() != components;
            scanned = true;
        }
        if matches!(kind, 0xe0..=0xef | 0xfe) {
            (count, size) = (count + 1, size + length);
            if count > 64 || size > METADATA {
                return None;
            }
        }
        at += length;
        // A scan's data runs to the next marker but a restart; 0xFF 0x00 is a stuffed 0xFF.
        while kind == 0xda
            && let Some(next) = memchr::memchr(0xff, &bytes[at..])
        {
            at += next;
            match bytes.get(at + 1) {
                Some(0x00 | 0xd0..=0xd7) => at += 2,
                _ => break,
            }
        }
    }
}

// A still lossy WebP filling its RIFF exactly: `VP8 ` first, or `VP8X` without animation or
// reserved flags, at most one `ALPH` with uncompressed alpha, and then one `VP8 ` key frame the
// canvas's size. Other chunks
// (colour profile, metadata, unknown) are skipped unread when whole. Lossless `VP8L`, compressed
// alpha, animation and anything malformed are refused before a decoder sees them.
fn webp(bytes: &[u8]) -> bool {
    let riff = bytes
        .get(4..8)
        .map(|n| u32::from_le_bytes([n[0], n[1], n[2], n[3]]));
    if riff.map(|n| n as usize + 8) != Some(bytes.len()) {
        return false;
    }
    // Before any chunk, after `VP8X`, after its `ALPH`, after the image; the canvas `VP8X` names.
    let (mut at, mut state, mut canvas) = (12, 0, None);
    while at < bytes.len() {
        let Some(head) = bytes.get(at..at + 8) else {
            return false;
        };
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if at + 8 + size + size % 2 > bytes.len() {
            return false;
        }
        let body = &bytes[at + 8..at + 8 + size];
        let first = body.first().copied();
        let int = |at: usize, n: usize| {
            (body[at..at + n].iter().rev()).fold(0, |v, b| v << 8 | u32::from(*b))
        };
        // A key frame's width and height, which the decoder would allocate before it compares them
        // with the canvas.
        let frame = (size >= 10 && body[0] & 1 == 0 && body[3..6] == [0x9d, 0x01, 0x2a])
            .then(|| (int(6, 2) & 0x3fff, int(8, 2) & 0x3fff));
        state = match (state, &head[..4]) {
            (0, b"VP8 ") if frame.is_some() => 3,
            (1 | 2, b"VP8 ") if frame.is_some() && frame == canvas => 3,
            (0, b"VP8X") if size == 10 && first.is_some_and(|f| f & !0x3c == 0) => {
                canvas = Some((int(4, 3) + 1, int(7, 3) + 1));
                1
            }
            (1, b"ALPH") if first.is_some_and(|f| f & 0xc3 == 0) => 2,
            (0, _) | (_, b"VP8 " | b"VP8X" | b"VP8L" | b"ALPH" | b"ANIM" | b"ANMF") => {
                return false;
            }
            (state, _) => state,
        };
        at += 8 + size + size % 2;
    }
    state == 3
}
