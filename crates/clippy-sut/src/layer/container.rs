//! The `.layer` container: an 8-byte magic, then PNG-like chunks `HEAD`, `dATA` …, `TAIL`, each
//! `u32` LE length, 4-byte type, data, CRC-32 (LE) over type and data.
//!
//! The `dATA` chunks hold one SQLite database image in order. Each starts with a `u16` LE flag:
//! 0 = the bytes follow in the clear; 1 = Clip Studio Paint has encrypted that part (observed:
//! always the first 5 120 bytes, plus 8 trailing bytes). This crate never decrypts; it reads the
//! database pages that are in the clear and treats the encrypted range as missing.

use crate::bytes::Reader;
use crate::{Limits, Result, check_limit, malformed};

pub(crate) const MAGIC: &[u8; 8] = b"\x89C2F\r\n\x1a\n";
const WHAT: &str = "layer container";
const FLAG_PLAIN: u16 = 0;
const FLAG_ENCRYPTED: u16 = 1;
/// Encrypted chunks carry 8 bytes beyond the database range they cover.
const ENCRYPTED_EXTRA: usize = 8;
const MAX_CHUNKS: usize = 4096;

/// The database image of a `.layer`: byte ranges in the clear, positioned in the image.
pub(crate) struct DbImage {
    /// (image offset, bytes) for every plain range.
    pub plain: Vec<(usize, Vec<u8>)>,
    /// Total image length (plain and encrypted).
    pub len: usize,
}

impl DbImage {
    /// The bytes `[offset, offset + n)` if they are all in one plain range.
    pub fn get(&self, offset: usize, n: usize) -> Option<&[u8]> {
        let end = offset.checked_add(n)?;
        self.plain.iter().find_map(|(start, bytes)| {
            let rel = offset.checked_sub(*start)?;
            let rel_end = end.checked_sub(*start)?;
            bytes.get(rel..rel_end)
        })
    }

    /// Image offset of the first plain byte (0 for a file without encryption).
    pub fn first_plain(&self) -> Option<usize> {
        self.plain.iter().map(|(o, _)| *o).min()
    }
}

/// Splits a `.layer` file into its database image.
pub(crate) fn read(bytes: &[u8], limits: &Limits) -> Result<DbImage> {
    check_limit("layer file size", bytes.len() as u64, limits.max_file_bytes)?;
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return malformed(WHAT, "not a .layer file");
    }
    let mut r = Reader::at(bytes, 8, WHAT);
    let mut image = DbImage { plain: Vec::new(), len: 0 };
    let mut chunks = 0;
    let mut tail = false;
    while !r.is_empty() {
        chunks += 1;
        if chunks > MAX_CHUNKS {
            return malformed(WHAT, "too many chunks");
        }
        let len = usize::try_from(r.u32_le()?).unwrap_or(usize::MAX);
        let kind = r.array::<4>()?;
        let data = r.take(len)?;
        let crc = r.u32_le()?;
        let mut hasher = crc32(&kind, 0xffff_ffff);
        hasher = crc32(data, hasher);
        if crc != !hasher {
            return malformed(WHAT, format!("CRC mismatch in chunk {}", String::from_utf8_lossy(&kind)));
        }
        match &kind {
            b"dATA" => {
                let mut d = Reader::new(data, WHAT);
                match u16::from_le_bytes(d.array()?) {
                    FLAG_PLAIN => {
                        let body = d.take(d.remaining())?;
                        image.plain.push((image.len, body.to_vec()));
                        image.len = image.len.saturating_add(body.len());
                    }
                    FLAG_ENCRYPTED => {
                        let covered = d.remaining().saturating_sub(ENCRYPTED_EXTRA) / 512 * 512;
                        image.len = image.len.saturating_add(covered);
                    }
                    flag => return malformed(WHAT, format!("dATA flag {flag}")),
                }
            }
            b"TAIL" => {
                tail = true;
                break;
            }
            _ => {}
        }
    }
    if !tail {
        return malformed(WHAT, "no TAIL chunk (truncated)");
    }
    if image.plain.is_empty() {
        return malformed(WHAT, "no readable data");
    }
    Ok(image)
}

/// Builds a `.layer` file around a database image stored in the clear.
pub(crate) fn write(db: &[u8]) -> Result<Vec<u8>> {
    let mut out = MAGIC.to_vec();
    chunk(&mut out, b"HEAD", &[])?;
    let mut data = FLAG_PLAIN.to_le_bytes().to_vec();
    data.extend_from_slice(db);
    chunk(&mut out, b"dATA", &data)?;
    chunk(&mut out, b"TAIL", &[])?;
    Ok(out)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<()> {
    let len = u32::try_from(data.len()).or_else(|_| malformed(WHAT, "chunk above 4 GiB"))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = !crc32(data, crc32(kind, 0xffff_ffff));
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(())
}

/// CRC-32 (IEEE, reflected), continued from `crc` (start with `0xffff_ffff`, invert at the end).
fn crc32(bytes: &[u8], mut crc: u32) -> u32 {
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_png_reference_value() {
        // CRC of an empty IEND chunk, as in every PNG file.
        assert_eq!(!crc32(b"IEND", 0xffff_ffff), 0xae42_6082);
    }

    #[test]
    fn plain_containers_round_trip_and_truncations_fail() {
        let file = write(&[7u8; 3000]).unwrap();
        let image = read(&file, &Limits::default()).unwrap();
        assert_eq!(image.len, 3000);
        assert_eq!(image.get(10, 5), Some(&[7u8; 5][..]));
        for cut in 0..file.len() {
            assert!(read(&file[..cut], &Limits::default()).is_err(), "cut {cut}");
        }
        let mut bad = file.clone();
        bad[40] ^= 1;
        assert!(read(&bad, &Limits::default()).is_err());
    }
}
