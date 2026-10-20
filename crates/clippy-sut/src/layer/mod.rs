//! Brush tip materials (`.layer`): decoding to RGBA and writing RGBA back.
//!
//! A `.layer` is a small Clip Studio Paint document: a container ([`container`]) around an SQLite
//! database whose `Offscreen` rows hold tiled bitmaps ([`tiles`]). The database header and schema
//! pages are encrypted by Clip Studio Paint, so [`decode`] reads the clear pages directly
//! ([`pages`]) and picks the `Offscreen` row that holds the tip. Nothing is decrypted.
//!
//! [`encode`] writes a tip as a `.layer` whose database is entirely in the clear. This crate reads
//! such files back losslessly; whether Clip Studio Paint accepts them is not verified yet (crate
//! README, "Open questions").

mod container;
mod pages;
mod tiles;
#[cfg(not(target_arch = "wasm32"))]
mod write;

pub use tiles::TipFormat;
#[cfg(not(target_arch = "wasm32"))]
pub use write::encode;

use crate::{Limits, Result, malformed};
use pages::{Field, Pages};
use tiles::{Attributes, BlockData};

/// Page sizes tried when the database header is encrypted (observed: 1 024 and 4 096).
const PAGE_SIZES: [usize; 7] = [1024, 4096, 2048, 8192, 16_384, 32_768, 65_536];

/// A decoded brush tip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight RGBA, row-major. Alpha is the tip's coverage; mask-only tips are black.
    pub rgba: Vec<u8>,
    /// The layout the file stores.
    pub format: TipFormat,
}

impl Tip {
    /// Coverage of each pixel, assuming Clip Studio Paint paints with darkness × alpha (black
    /// fully, white not at all; mask-only tips give their alpha). Unverified until M8 compares
    /// strokes with Clip Studio Paint.
    pub fn coverage(&self) -> Vec<u8> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let lum = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
                ((255 - lum) * u32::from(p[3]) / 255) as u8
            })
            .collect()
    }
}

/// Decodes a `.layer` file to RGBA.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Tip> {
    let image = container::read(bytes, limits)?;
    let sizes: Vec<usize> = match image.first_plain() {
        Some(0) => Pages::header_page_size(&image).into_iter().collect(),
        _ => PAGE_SIZES.to_vec(),
    };
    let max_payload = usize::try_from(limits.max_entry_bytes).unwrap_or(usize::MAX);
    let mut last_error = None;
    for size in sizes {
        let Some(pages) = Pages::new(&image, size, max_payload) else { continue };
        let mut best: Option<((usize, u64), Attributes, BlockData)> = None;
        for (_, fields) in pages.records() {
            let [_, _, _, _, Field::Blob(attr), data] = fields.as_slice() else { continue };
            let Field::Blob(data) = data else { continue };
            let (Ok(attr), Ok(data)) = (Attributes::decode(attr), BlockData::decode(data)) else { continue };
            // The tip is the bitmap with the most stored tiles; folder layers and unused mipmaps have
            // none. A tip without any stored tile (fully transparent) falls back to the largest one.
            let key = (data.present(), u64::from(attr.width) * u64::from(attr.height));
            if best.as_ref().is_none_or(|(k, _, _)| key > *k) {
                best = Some((key, attr, data));
            }
        }
        if let Some((_, attr, data)) = best {
            match tiles::to_rgba(&attr, &data, limits) {
                Ok((format, rgba)) => return Ok(Tip { width: attr.width, height: attr.height, rgba, format }),
                Err(e) => last_error = Some(e),
            }
        }
    }
    match last_error {
        Some(e) => Err(e),
        None => malformed("layer", "no readable bitmap"),
    }
}

/// Whether `bytes` start like a `.layer` file.
pub fn is_layer(bytes: &[u8]) -> bool {
    bytes.starts_with(container::MAGIC)
}
