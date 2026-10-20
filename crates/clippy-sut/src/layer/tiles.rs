//! `Offscreen.Attribute` and `Offscreen.BlockData`: the tiled bitmap of a layer, the same layout
//! as in Clip Studio Paint projects (`.clip`). Layout as documented by clipfile-rs
//! (`docs/format-analysis.md`, MIT) and checked against all 176 tips of a Clip Studio Paint 1.5
//! installation; older tips lack `BlockStatus` and `BlockCheckSum`.

use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::bytes::{Reader, put_label, put_len, put_u32};
use crate::{Limits, Result, check_limit, malformed};

const ATTR: &str = "layer attributes";
const BLOCKS: &str = "layer tiles";
const BEGIN: &str = "BlockDataBeginChunk";
const END: &str = "BlockDataEndChunk";
const STATUS: &str = "BlockStatus";
const CHECKSUM: &str = "BlockCheckSum";
const MAX_TILE_SIDE: u32 = 4096;
const MAX_BLOCKS: usize = 1 << 20;

/// `Offscreen.Attribute`: bitmap size, tile grid, pixel packing, fill value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Attributes {
    pub width: u32,
    pub height: u32,
    pub grid_width: u32,
    pub grid_height: u32,
    /// Packing words (16; 14 in older tips): [1] alpha channels, [2] buffer channels, [3] total,
    /// [6] >> 5 buffer bits per pixel, [8] >> 5 alpha bits, [10]/[11] tile width/height.
    pub packing: Vec<u32>,
    pub fill: u32,
    pub fill_unknown: u32,
    pub init_colors: Vec<u32>,
    pub block_sizes: Vec<u32>,
}

impl Attributes {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, ATTR);
        let (header, param, init, block) = (r.len_be()?, r.len_be()?, r.len_be()?, r.len_be()?);
        let total = header.checked_add(param).and_then(|v| v.checked_add(init)).and_then(|v| v.checked_add(block));
        if header != 16 || total != Some(bytes.len()) {
            return malformed(ATTR, "section sizes do not add up");
        }
        let mut p = Reader::new(r.take(param)?, ATTR);
        p.expect_label("Parameter")?;
        let (width, height, grid_width, grid_height) = (p.u32_be()?, p.u32_be()?, p.u32_be()?, p.u32_be()?);
        if !p.remaining().is_multiple_of(4) || p.remaining() / 4 < 12 || p.remaining() / 4 > 64 {
            return malformed(ATTR, format!("{} packing bytes", p.remaining()));
        }
        let packing = (0..p.remaining() / 4).map(|_| p.u32_be()).collect::<Result<Vec<_>>>()?;
        let mut c = Reader::new(r.take(init)?, ATTR);
        c.expect_label("InitColor")?;
        let (record, fill, fill_unknown, count, size) = (c.u32_be()?, c.u32_be()?, c.u32_be()?, c.len_be()?, c.u32_be()?);
        if record != 20 || size != 4 || count.saturating_mul(4) > c.remaining() {
            return malformed(ATTR, "InitColor layout");
        }
        let init_colors = (0..count).map(|_| c.u32_be()).collect::<Result<_>>()?;
        let mut b = Reader::new(r.take(block)?, ATTR);
        b.expect_label("BlockSize")?;
        let (record, count, size) = (b.u32_be()?, b.len_be()?, b.u32_be()?);
        if record != 12 || size != 4 || count.saturating_mul(4) > b.remaining() {
            return malformed(ATTR, "BlockSize layout");
        }
        if u64::from(grid_width) * u64::from(grid_height) != count as u64 {
            return malformed(ATTR, "BlockSize count does not match the grid");
        }
        let block_sizes = (0..count).map(|_| b.u32_be()).collect::<Result<_>>()?;
        Ok(Self { width, height, grid_width, grid_height, packing, fill, fill_unknown, init_colors, block_sizes })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut param = Vec::new();
        put_label(&mut param, "Parameter");
        for v in [self.width, self.height, self.grid_width, self.grid_height].iter().chain(&self.packing) {
            put_u32(&mut param, *v);
        }
        let mut init = Vec::new();
        put_label(&mut init, "InitColor");
        for v in [20, self.fill, self.fill_unknown] {
            put_u32(&mut init, v);
        }
        put_len(&mut init, self.init_colors.len(), "init colors")?;
        put_u32(&mut init, 4);
        self.init_colors.iter().for_each(|v| put_u32(&mut init, *v));
        let mut block = Vec::new();
        put_label(&mut block, "BlockSize");
        put_u32(&mut block, 12);
        put_len(&mut block, self.block_sizes.len(), "block sizes")?;
        put_u32(&mut block, 4);
        self.block_sizes.iter().for_each(|v| put_u32(&mut block, *v));
        let mut out = Vec::new();
        put_u32(&mut out, 16);
        for s in [&param, &init, &block] {
            put_len(&mut out, s.len(), "attribute section")?;
        }
        for s in [param, init, block] {
            out.extend_from_slice(&s);
        }
        Ok(out)
    }

    fn word(&self, i: usize) -> u32 {
        self.packing.get(i).copied().unwrap_or(0)
    }

    pub fn tile_width(&self) -> u32 {
        self.word(10)
    }

    pub fn tile_height(&self) -> u32 {
        self.word(11)
    }

    pub fn alpha_channels(&self) -> u32 {
        self.word(1)
    }

    pub fn buffer_channels(&self) -> u32 {
        self.word(2)
    }

    pub fn buffer_bits(&self) -> u32 {
        self.word(6) >> 5
    }

    pub fn alpha_bits(&self) -> u32 {
        self.word(8) >> 5
    }
}

/// One block of `Offscreen.BlockData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub index: u32,
    /// The 12-byte block parameters (`u16` channels, `u16` reserved, `u32` width, `u32` height).
    pub params: [u8; 12],
    /// The zlib stream, if the tile has pixels.
    pub data: Option<Vec<u8>>,
}

/// `Offscreen.BlockData`: the blocks, then optional status and checksum arrays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockData {
    pub blocks: Vec<Block>,
    pub status: Option<Vec<u32>>,
    pub checksums: Option<Vec<u32>>,
}

impl BlockData {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, BLOCKS);
        let mut blocks = Vec::new();
        while r.remaining() >= 8 && Reader::at(bytes, r.pos() + 4, BLOCKS).peek_label(BEGIN) {
            if blocks.len() >= MAX_BLOCKS {
                return malformed(BLOCKS, "too many blocks");
            }
            let start = r.pos();
            let len = r.len_be()?;
            r.expect_label(BEGIN)?;
            let index = r.u32_be()?;
            let params = r.array::<12>()?;
            let data = match r.u32_be()? {
                0 => None,
                _ => {
                    let outer = r.len_be()?;
                    let inner = usize::try_from(r.u32_le()?).unwrap_or(usize::MAX);
                    if inner.checked_add(4) != Some(outer) {
                        return malformed(BLOCKS, "block data lengths disagree");
                    }
                    Some(r.take(inner)?.to_vec())
                }
            };
            r.expect_label(END)?;
            if start.checked_add(len) != Some(r.pos()) {
                return malformed(BLOCKS, "block length does not match its content");
            }
            blocks.push(Block { index, params, data });
        }
        let status = if r.is_empty() { None } else { Some(array(&mut r, STATUS)?) };
        let checksums = if r.is_empty() { None } else { Some(array(&mut r, CHECKSUM)?) };
        if !r.is_empty() {
            return malformed(BLOCKS, "bytes after the checksums");
        }
        Ok(Self { blocks, status, checksums })
    }

    /// Encodes the blocks; returns the bytes and each block's encoded length (`BlockSize`).
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u32>)> {
        let mut out = Vec::new();
        let mut sizes = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            let mut body = Vec::new();
            put_label(&mut body, BEGIN);
            put_u32(&mut body, b.index);
            body.extend_from_slice(&b.params);
            match &b.data {
                None => put_u32(&mut body, 0),
                Some(z) => {
                    put_u32(&mut body, 1);
                    put_len(&mut body, z.len().saturating_add(4), "tile size")?;
                    body.extend_from_slice(&u32::try_from(z.len()).unwrap_or(u32::MAX).to_le_bytes());
                    body.extend_from_slice(z);
                }
            }
            put_label(&mut body, END);
            let len = body.len().saturating_add(4);
            put_len(&mut out, len, "block length")?;
            out.extend_from_slice(&body);
            sizes.push(u32::try_from(len).unwrap_or(u32::MAX));
        }
        for (label, values) in [(STATUS, &self.status), (CHECKSUM, &self.checksums)] {
            let Some(values) = values else { break };
            put_label(&mut out, label);
            put_u32(&mut out, 12);
            put_len(&mut out, values.len(), "status count")?;
            put_u32(&mut out, 4);
            values.iter().for_each(|v| put_u32(&mut out, *v));
        }
        Ok((out, sizes))
    }

    pub fn present(&self) -> usize {
        self.blocks.iter().filter(|b| b.data.is_some()).count()
    }
}

fn array(r: &mut Reader<'_>, label: &str) -> Result<Vec<u32>> {
    r.expect_label(label)?;
    let (record, count, size) = (r.u32_be()?, r.len_be()?, r.u32_be()?);
    if record != 12 || size != 4 || count.saturating_mul(4) > r.remaining() {
        return malformed(BLOCKS, format!("{label} layout"));
    }
    (0..count).map(|_| r.u32_be()).collect()
}

/// Pixel layouts observed in brush tips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipFormat {
    /// 8-bit alpha only: the tip is a mask.
    Alpha8,
    /// 8-bit gray value plus 8-bit alpha.
    GrayAlpha8,
    /// 1-bit alpha only.
    Alpha1,
    /// 1-bit value plus 1-bit alpha.
    GrayAlpha1,
    /// 8-bit alpha plus B, G, R and an unused byte.
    Rgba8,
}

impl TipFormat {
    pub(crate) fn of(a: &Attributes) -> Result<Self> {
        let key = (a.alpha_channels(), a.alpha_bits(), a.buffer_channels(), a.buffer_bits());
        Ok(match key {
            (1, 8, 0, _) => TipFormat::Alpha8,
            (1, 8, 1, 8) => TipFormat::GrayAlpha8,
            (1, 1, 0, _) => TipFormat::Alpha1,
            (1, 1, 1, 1) => TipFormat::GrayAlpha1,
            (1, 8, 4, 32) => TipFormat::Rgba8,
            _ => {
                return Err(crate::Error::Unsupported { what: "tip pixel packing", reason: format!("{key:?}") });
            }
        })
    }

    /// Decompressed bytes of one tile.
    fn tile_bytes(self, pixels: usize) -> Option<usize> {
        match self {
            TipFormat::Alpha8 => Some(pixels),
            TipFormat::GrayAlpha8 => pixels.checked_mul(2),
            TipFormat::Alpha1 => Some(pixels / 8),
            TipFormat::GrayAlpha1 => Some(pixels / 4),
            TipFormat::Rgba8 => pixels.checked_mul(5),
        }
    }
}

/// Assembles the tiles into straight (non-premultiplied) RGBA rows.
pub(crate) fn to_rgba(a: &Attributes, data: &BlockData, limits: &Limits) -> Result<(TipFormat, Vec<u8>)> {
    let format = TipFormat::of(a)?;
    let (tw, th) = (a.tile_width(), a.tile_height());
    for (what, v) in [("tip width", a.width), ("tip height", a.height)] {
        check_limit(what, u64::from(v), u64::from(limits.max_tip_dimension))?;
    }
    check_limit("tip pixels", u64::from(a.width) * u64::from(a.height), limits.max_tip_pixels)?;
    if tw == 0 || th == 0 || tw > MAX_TILE_SIDE || th > MAX_TILE_SIDE || tw % 8 != 0 || a.width == 0 || a.height == 0 {
        return malformed(ATTR, format!("tile {tw}×{th}, bitmap {}×{}", a.width, a.height));
    }
    if u64::from(a.grid_width) * u64::from(tw) < u64::from(a.width) || u64::from(a.grid_height) * u64::from(th) < u64::from(a.height) {
        return malformed(ATTR, "tile grid smaller than the bitmap");
    }
    let (w, h) = (a.width as usize, a.height as usize);
    let tile_px = tw as usize * th as usize;
    let expected = format.tile_bytes(tile_px).ok_or_else(|| crate::Error::Malformed { what: ATTR, reason: "tile size".into() })?;
    check_limit("tile size", expected as u64, limits.max_entry_bytes)?;
    let fill = if a.fill == 0 { 0 } else { 255 };
    let mut rgba = vec![0u8; w * h * 4];
    for px in rgba.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&match format {
            TipFormat::Alpha8 | TipFormat::Alpha1 => [0, 0, 0, fill],
            _ => [fill, fill, fill, fill],
        });
    }
    let mut tile = Vec::with_capacity(expected);
    for block in &data.blocks {
        let Some(z) = &block.data else { continue };
        if block.index >= a.grid_width.saturating_mul(a.grid_height) {
            return malformed(BLOCKS, format!("tile index {}", block.index));
        }
        tile.clear();
        ZlibDecoder::new(z.as_slice())
            .take(expected as u64 + 1)
            .read_to_end(&mut tile)
            .map_err(|e| crate::Error::Malformed { what: BLOCKS, reason: format!("tile {}: {e}", block.index) })?;
        if tile.len() != expected {
            return malformed(BLOCKS, format!("tile {} has {} bytes, expected {expected}", block.index, tile.len()));
        }
        let (gx, gy) = ((block.index % a.grid_width) as usize, (block.index / a.grid_width) as usize);
        let (x0, y0) = (gx * tw as usize, gy * th as usize);
        for ty in 0..th as usize {
            let y = y0 + ty;
            if y >= h {
                break;
            }
            for tx in 0..tw as usize {
                let x = x0 + tx;
                if x >= w {
                    break;
                }
                let i = ty * tw as usize + tx;
                let bit = |plane: usize| -> u8 {
                    let byte = tile.get(plane + i / 8).copied().unwrap_or(0);
                    if byte & (0x80 >> (i % 8)) != 0 { 255 } else { 0 }
                };
                let at = |o: usize| tile.get(o).copied().unwrap_or(0);
                let px = match format {
                    TipFormat::Alpha8 => [0, 0, 0, at(i)],
                    TipFormat::GrayAlpha8 => {
                        let v = at(tile_px + i);
                        [v, v, v, at(i)]
                    }
                    TipFormat::Alpha1 => [0, 0, 0, bit(0)],
                    TipFormat::GrayAlpha1 => {
                        let v = bit(tile_px / 8);
                        [v, v, v, bit(0)]
                    }
                    TipFormat::Rgba8 => {
                        let o = tile_px + i * 4;
                        [at(o + 2), at(o + 1), at(o), at(i)]
                    }
                };
                let o = (y * w + x) * 4;
                if let Some(dst) = rgba.get_mut(o..o + 4) {
                    dst.copy_from_slice(&px);
                }
            }
        }
    }
    Ok((format, rgba))
}

/// Adler-32 of `bytes`, continued from `a` (start with 1).
pub(crate) fn adler32(bytes: &[u8], a: u32) -> u32 {
    let (mut s1, mut s2) = (a & 0xffff, a >> 16);
    for chunk in bytes.chunks(5552) {
        for &b in chunk {
            s1 += u32::from(b);
            s2 += s1;
        }
        s1 %= 65_521;
        s2 %= 65_521;
    }
    (s2 << 16) | s1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adler_matches_the_reference() {
        assert_eq!(adler32(b"Wikipedia", 1), 0x11e6_0398);
    }

    #[test]
    fn block_data_round_trips_with_and_without_trailers() {
        let blocks = vec![
            Block { index: 0, params: [0, 2, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0], data: None },
            Block { index: 1, params: [0, 2, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0], data: Some(vec![0x78, 1, 2, 3]) },
        ];
        for (status, checksums) in [(None, None), (Some(vec![1, 1]), None), (Some(vec![1, 1]), Some(vec![0, 7]))] {
            let bd = BlockData { blocks: blocks.clone(), status, checksums };
            let (bytes, sizes) = bd.encode().unwrap();
            assert_eq!(sizes[0], 0x68);
            assert_eq!(BlockData::decode(&bytes).unwrap(), bd);
        }
    }
}
