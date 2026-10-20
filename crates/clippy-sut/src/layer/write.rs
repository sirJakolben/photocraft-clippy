//! RGBA → `.layer`: a plain (unencrypted) database with the tables a tip needs, laid out like the
//! observed files (one folder layer, one raster layer, its mipmap, one `Offscreen`).

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use rusqlite::{Connection, params};

use super::tiles::{Attributes, Block, BlockData, TipFormat, adler32};
use super::{Tip, container};
use crate::{Result, malformed};

const TILE: u32 = 256;
const SCHEMA: &str = "
CREATE TABLE Canvas(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasUnit INTEGER DEFAULT NULL, CanvasWidth REAL DEFAULT NULL, CanvasHeight REAL DEFAULT NULL, CanvasResolution REAL DEFAULT NULL, CanvasChannelBytes INTEGER DEFAULT NULL, CanvasDefaultChannelOrder INTEGER DEFAULT NULL, CanvasRootFolder INTEGER DEFAULT NULL, CanvasCurrentLayer INTEGER DEFAULT NULL);
CREATE TABLE Layer(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerName TEXT DEFAULT NULL, LayerType INTEGER DEFAULT NULL, LayerOpacity INTEGER DEFAULT NULL, LayerFolder INTEGER DEFAULT NULL, LayerVisibility INTEGER DEFAULT NULL, LayerNextIndex INTEGER DEFAULT NULL, LayerFirstChildIndex INTEGER DEFAULT NULL, LayerRenderMipmap INTEGER DEFAULT NULL);
CREATE TABLE Mipmap(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, MipmapCount INTEGER DEFAULT NULL, BaseMipmapInfo INTEGER DEFAULT NULL);
CREATE TABLE MipmapInfo(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, ThisScale REAL DEFAULT NULL, Offscreen INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL);
CREATE TABLE Offscreen(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, Attribute BLOB DEFAULT NULL, BlockData BLOB DEFAULT NULL);
";

/// Encodes a tip as a `.layer`. Gray tips (R = G = B everywhere) are stored as gray + alpha, others
/// as alpha + BGR. The database is in the clear (see the module docs of [`super`]).
pub fn encode(tip: &Tip) -> Result<Vec<u8>> {
    let (w, h) = (tip.width, tip.height);
    if w == 0 || h == 0 || tip.rgba.len() as u64 != u64::from(w) * u64::from(h) * 4 {
        return malformed("tip", "size does not match the pixels");
    }
    let gray = tip.rgba.as_chunks::<4>().0.iter().all(|p| p[0] == p[1] && p[1] == p[2]);
    let format = if gray { TipFormat::GrayAlpha8 } else { TipFormat::Rgba8 };
    let (gw, gh) = (w.div_ceil(TILE), h.div_ceil(TILE));
    let channels: u8 = if gray { 2 } else { 5 };
    let mut params_bytes = [0u8; 12];
    params_bytes[1] = channels;
    params_bytes[4..8].copy_from_slice(&TILE.to_be_bytes());
    params_bytes[8..12].copy_from_slice(&TILE.to_be_bytes());

    let mut blocks = Vec::new();
    let mut checksums = Vec::new();
    for index in 0..gw * gh {
        let (x0, y0) = ((index % gw) * TILE, (index / gw) * TILE);
        let tile = tile_bytes(tip, x0, y0, format);
        let data = if tile.iter().all(|&b| b == 0) {
            checksums.push(0);
            None
        } else {
            let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
            z.write_all(&tile)?;
            let z = z.finish()?;
            let len = u32::try_from(z.len()).unwrap_or(u32::MAX).to_le_bytes();
            checksums.push(adler32(&z, adler32(&len, 1)));
            Some(z)
        };
        blocks.push(Block { index, params: params_bytes, data });
    }
    let status = vec![1; blocks.len()];
    let block_data = BlockData { blocks, status: Some(status), checksums: Some(checksums) };
    let (block_bytes, block_sizes) = block_data.encode()?;
    let packing = if gray {
        [17, 1, 1, 2, 65_536, 1, 256, 1, 256, 65_536, TILE, TILE, 8, 8, 0, 0]
    } else {
        [33, 1, 4, 5, 65_536, 4, 1024, 1, 256, 65_536, TILE, TILE, 8, 8, 0, 0]
    };
    let attributes = Attributes {
        width: w,
        height: h,
        grid_width: gw,
        grid_height: gh,
        packing: packing.to_vec(),
        fill: 0,
        fill_unknown: 0,
        init_colors: Vec::new(),
        block_sizes,
    }
    .encode()?;

    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA page_size = 4096;")?;
    conn.execute_batch(SCHEMA)?;
    conn.execute("INSERT INTO Canvas VALUES (1, 1, 0, ?1, ?2, 72.0, 1, 0, 2, 3)", params![f64::from(w), f64::from(h)])?;
    conn.execute_batch(
        "INSERT INTO Layer VALUES (1, 2, 0, '', 256, 256, 1, 1, 0, 3, 0);
         INSERT INTO Layer VALUES (2, 3, 0, '', 0, 256, 0, 1, 0, 0, 4);
         INSERT INTO Mipmap VALUES (1, 4, 0, 3, 1, 5);
         INSERT INTO MipmapInfo VALUES (1, 5, 0, 3, 100.0, 6, 0);",
    )?;
    conn.execute("INSERT INTO Offscreen VALUES (1, 6, 0, 3, ?1, ?2)", params![attributes, block_bytes])?;
    let db = conn.serialize("main")?.to_vec();
    container::write(&db)
}

/// One tile in the native planar layout: the alpha plane, then gray or interleaved B, G, R, 0.
fn tile_bytes(tip: &Tip, x0: u32, y0: u32, format: TipFormat) -> Vec<u8> {
    let px = (TILE * TILE) as usize;
    let gray = format == TipFormat::GrayAlpha8;
    let mut out = vec![0u8; if gray { px * 2 } else { px * 5 }];
    for ty in 0..TILE {
        for tx in 0..TILE {
            let (x, y) = (x0 + tx, y0 + ty);
            if x >= tip.width || y >= tip.height {
                continue;
            }
            let src = ((y as usize) * tip.width as usize + x as usize) * 4;
            let Some(&[r, g, b, a]) = tip.rgba.get(src..src + 4).and_then(|s| <&[u8; 4]>::try_from(s).ok()) else {
                continue;
            };
            let i = (ty * TILE + tx) as usize;
            out[i] = a;
            if gray {
                out[px + i] = r;
            } else {
                out[px + i * 4..px + i * 4 + 3].copy_from_slice(&[b, g, r]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;

    fn tip(w: u32, h: u32, colored: bool) -> Tip {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = ((x * 7 + y * 3) % 256) as u8;
                rgba.extend_from_slice(&[v, if colored { 255 - v } else { v }, v, ((x + y) % 251) as u8]);
            }
        }
        Tip { width: w, height: h, rgba, format: TipFormat::GrayAlpha8 }
    }

    #[test]
    fn encoded_tips_decode_to_the_same_pixels() {
        for (w, h, colored) in [(1, 1, false), (300, 20, false), (257, 513, true)] {
            let t = tip(w, h, colored);
            let bytes = encode(&t).unwrap();
            let back = super::super::decode(&bytes, &Limits::default()).unwrap();
            assert_eq!((back.width, back.height), (w, h));
            assert_eq!(back.format, if colored { TipFormat::Rgba8 } else { TipFormat::GrayAlpha8 });
            assert_eq!(back.rgba, t.rgba, "{w}×{h}");
        }
    }

    #[test]
    fn wrong_sizes_are_errors() {
        let mut t = tip(4, 4, false);
        t.rgba.pop();
        assert!(encode(&t).is_err());
        assert!(encode(&Tip { width: 0, height: 0, rgba: Vec::new(), format: TipFormat::Alpha8 }).is_err());
    }
}
