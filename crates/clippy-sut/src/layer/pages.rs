//! A minimal read-only reader for SQLite table b-tree pages (file format: sqlite.org/fileformat.html).
//!
//! A `.layer` database cannot be opened with SQLite: its first pages (header and schema) are
//! encrypted. The pages after them are in the clear, so this reader scans every readable leaf page
//! for records and follows overflow chains, without needing the schema. Every page number, offset
//! and length from the file is checked; chains are bounded by a visited set.

use std::collections::HashSet;

use super::container::DbImage;

const LEAF_TABLE: u8 = 0x0d;
const HEADER_LEN: usize = 100;

/// One column of a record.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Field {
    Null,
    Int(i64),
    Real(f64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

/// The pages of a database image at one page size.
pub(crate) struct Pages<'a> {
    image: &'a DbImage,
    page_size: usize,
    usable: usize,
    max_payload: usize,
}

impl<'a> Pages<'a> {
    /// The image read with `page_size`; `None` if the image is not a whole number of pages.
    pub fn new(image: &'a DbImage, page_size: usize, max_payload: usize) -> Option<Self> {
        if !(512..=65_536).contains(&page_size) || !page_size.is_power_of_two() || image.len % page_size != 0 {
            return None;
        }
        // The reserved-bytes count lives in the database header, which is usually encrypted;
        // Clip Studio Paint does not reserve any.
        let reserved = image.get(20, 1).and_then(|b| b.first().copied()).map_or(0, usize::from);
        let usable = page_size.checked_sub(reserved).filter(|&u| u >= 480)?;
        Some(Self { image, page_size, usable, max_payload })
    }

    /// The page size stored in a plain database header (`u16` BE at offset 16; 1 means 65 536).
    pub fn header_page_size(image: &DbImage) -> Option<usize> {
        let b = image.get(16, 2)?;
        match u16::from_be_bytes([*b.first()?, *b.get(1)?]) {
            1 => Some(65_536),
            v => Some(usize::from(v)),
        }
    }

    /// Number of pages.
    pub fn count(&self) -> usize {
        self.image.len / self.page_size
    }

    /// Page `n` (1-based), if it is entirely in the clear.
    pub fn page(&self, n: usize) -> Option<&'a [u8]> {
        let offset = n.checked_sub(1)?.checked_mul(self.page_size)?;
        self.image.get(offset, self.page_size)
    }

    /// Every record of every readable table leaf page, as (rowid, fields). Cells that do not parse,
    /// or whose overflow chain runs into an unreadable page, are skipped.
    pub fn records(&self) -> Vec<(i64, Vec<Field>)> {
        let mut out = Vec::new();
        for n in 1..=self.count() {
            let Some(page) = self.page(n) else { continue };
            let header = if n == 1 { HEADER_LEN } else { 0 };
            if page.get(header) != Some(&LEAF_TABLE) {
                continue;
            }
            let Some(cells) = be16(page, header + 3) else { continue };
            for i in 0..usize::from(cells) {
                let Some(ptr) = be16(page, header + 8 + 2 * i) else { break };
                if let Some(record) = self.cell(page, usize::from(ptr)) {
                    out.push(record);
                }
            }
        }
        out
    }

    fn cell(&self, page: &[u8], offset: usize) -> Option<(i64, Vec<Field>)> {
        let (payload_len, o) = varint(page, offset)?;
        let (rowid, o) = varint(page, o)?;
        let payload_len = usize::try_from(payload_len).ok().filter(|&p| p <= self.max_payload)?;
        let payload = self.payload(page, o, payload_len)?;
        Some((rowid as i64, record(&payload)?))
    }

    /// The payload of a cell starting at `offset`, with its overflow pages.
    fn payload(&self, page: &[u8], offset: usize, len: usize) -> Option<Vec<u8>> {
        let u = self.usable;
        let max_local = u - 35;
        if len <= max_local {
            return page.get(offset..offset.checked_add(len)?).map(<[u8]>::to_vec);
        }
        let min_local = (u - 12) * 32 / 255 - 23;
        let k = min_local + (len - min_local) % (u - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(page.get(offset..offset + local)?);
        let mut next = be32(page, offset + local)? as usize;
        let mut seen = HashSet::new();
        while out.len() < len {
            if next == 0 || !seen.insert(next) {
                return None;
            }
            let p = self.page(next)?;
            let take = (len - out.len()).min(u - 4);
            out.extend_from_slice(p.get(4..4 + take)?);
            next = be32(p, 0)? as usize;
        }
        Some(out)
    }
}

/// Decodes a record (header of serial types, then the values).
fn record(payload: &[u8]) -> Option<Vec<Field>> {
    let (header_len, mut h) = varint(payload, 0)?;
    let header_len = usize::try_from(header_len).ok().filter(|&l| l <= payload.len())?;
    let mut body = header_len;
    let mut fields = Vec::new();
    while h < header_len {
        let (serial, next) = varint(payload, h)?;
        h = next;
        let (field, size) = match serial {
            0 => (Field::Null, 0),
            1..=6 => {
                let size = [0, 1, 2, 3, 4, 6, 8][serial as usize];
                let bytes = payload.get(body..body + size)?;
                let mut v: i64 = if bytes.first().is_some_and(|&b| b & 0x80 != 0) { -1 } else { 0 };
                for &b in bytes {
                    v = (v << 8) | i64::from(b);
                }
                (Field::Int(v), size)
            }
            7 => {
                let bytes: [u8; 8] = payload.get(body..body + 8)?.try_into().ok()?;
                (Field::Real(f64::from_be_bytes(bytes)), 8)
            }
            8 => (Field::Int(0), 0),
            9 => (Field::Int(1), 0),
            10 | 11 => return None,
            s => {
                let size = usize::try_from((s - 12) / 2).ok()?;
                let bytes = payload.get(body..body.checked_add(size)?)?.to_vec();
                (if s % 2 == 0 { Field::Blob(bytes) } else { Field::Text(bytes) }, size)
            }
        };
        body += size;
        fields.push(field);
    }
    Some(fields)
}

fn varint(bytes: &[u8], offset: usize) -> Option<(u64, usize)> {
    let mut v: u64 = 0;
    for i in 0..9 {
        let b = *bytes.get(offset + i)?;
        if i == 8 {
            return Some(((v << 8) | u64::from(b), offset + 9));
        }
        v = (v << 7) | u64::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Some((v, offset + i + 1));
        }
    }
    None
}

fn be16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?))
}

fn be32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_records_decode() {
        assert_eq!(varint(&[0x7f], 0), Some((127, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
        assert_eq!(varint(&[0xff; 9], 0), Some((u64::MAX, 9)));
        assert_eq!(varint(&[0x81], 0), None);
        // Header: length 4, NULL, int8, blob of 2; body: -1, [9, 8].
        let rec = [4, 0, 1, 16, 0xff, 9, 8];
        assert_eq!(record(&rec), Some(vec![Field::Null, Field::Int(-1), Field::Blob(vec![9, 8])]));
        for cut in 0..rec.len() {
            assert!(record(&rec[..cut]).is_none() || cut == 0);
        }
    }
}
