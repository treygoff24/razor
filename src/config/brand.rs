//! Configured avatar files and the bounded PNG format shared with the terminal stage.
use super::Error;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

pub(crate) type Brand = crate::protocol::Brand<Avatar>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Avatar {
    file: std::path::PathBuf,
    #[serde(default)]
    pub(crate) pool: bool,
    #[serde(skip)]
    data: String,
}
impl crate::protocol::Brand<Avatar> {
    pub(super) fn load(&mut self) -> Vec<String> {
        let mut notices = Vec::new();
        let mut count = 0;
        self.avatars.retain(|name, avatar| {
            let load = || -> Result<Vec<u8>, Error> {
                if count >= 16 {
                    return Err(Error::Invalid(
                        "[brand.avatars] holds at most 16 avatars.".into(),
                    ));
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&avatar.file)
                    .and_then(|file| file.take(2049).read_to_end(&mut bytes))
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                avatar_pixels(&bytes)?;
                Ok(bytes)
            };
            match load() {
                Ok(bytes) => {
                    avatar.data = STANDARD.encode(bytes);
                    count += 1;
                    true
                }
                Err(e) => {
                    // The key, not the file it names: a settings notice repeats no value.
                    notices.push(format!("brand.avatars.{name}.file does not load: {e}"));
                    false
                }
            }
        });
        notices
    }
    pub(crate) fn draw(&self, id: &str) -> Option<String> {
        let pool: Vec<_> = self
            .avatars
            .iter()
            .filter(|(_, a)| a.pool)
            .map(|(n, _)| n)
            .collect();
        let crew = crate::protocol::FAMILIES.len();
        let draw = crate::protocol::family_hash(id) as usize % (crew + pool.len());
        pool.get(draw.checked_sub(crew)?).cloned().cloned()
    }
    pub(crate) fn shown(
        &self,
        residents: &std::collections::BTreeMap<crate::protocol::SessionId, super::Resident>,
    ) -> crate::protocol::Brand {
        crate::protocol::Brand {
            resident_avatars: residents
                .iter()
                .filter_map(|(id, r)| Some((id.clone(), r.avatar.clone()?)))
                .collect(),
            name: self.name.clone(),
            colours: self.colours.clone(),
            avatars: self
                .avatars
                .iter()
                .map(|(n, a)| (n.clone(), a.data.clone()))
                .collect(),
        }
    }
}

/// Four row-major 33 px RGBA cells: idle, blink, talk, sleep. Decode within fixed
/// allocation limits.
pub(crate) fn avatar_pixels(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    if bytes.len() > 2048 {
        return Err(Error::Invalid(
            "Sheet exceeds 2 KiB; use a smaller sheet.".into(),
        ));
    }
    let decoder = png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: 65536 });
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let info = reader.info();
    if (info.width, info.height, info.color_type, info.bit_depth)
        != (132, 33, png::ColorType::Rgba, png::BitDepth::Eight)
    {
        return Err(Error::Invalid("Use a 132x33 8-bit RGBA PNG.".into()));
    }
    let mut data = vec![0; 132 * 33 * 4];
    reader
        .next_frame(&mut data)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(data)
}
