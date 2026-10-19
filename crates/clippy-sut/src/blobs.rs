//! `VariantShowParam` and the material reference blobs (`BrushPatternImageArray`, `TextureImage`
//! and their `Dual…` twins).

use crate::bytes::{Reader, put_len, put_u32};
use crate::{Result, malformed};

const SHOW_WHAT: &str = "VariantShowParam";
const REFS_WHAT: &str = "material reference list";
/// Most entries accepted in one list; observed lists hold at most 40 ids and a few materials.
const MAX_ENTRIES: usize = 1 << 16;

/// `VariantShowParam`: the ids of the settings whose eye is switched on in the Tool Property palette
/// (`i32` 12, `i32` count, `i32` 4, then the ids, big-endian). Verified on all 420 variants of a
/// Clip Studio Paint 1.5 tool database; the id → setting table is open (crate README).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShowParams {
    /// Parameter ids in file order.
    pub ids: Vec<u32>,
}

impl ShowParams {
    /// Decodes the blob.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, SHOW_WHAT);
        let (header, count, size) = (r.u32_be()?, r.len_be()?, r.u32_be()?);
        if header != 12 || size != 4 {
            return malformed(SHOW_WHAT, format!("header {header}, element size {size}"));
        }
        if count > MAX_ENTRIES || count.saturating_mul(4) != r.remaining() {
            return malformed(SHOW_WHAT, format!("{count} ids in {} bytes", r.remaining()));
        }
        let ids = (0..count).map(|_| r.u32_be()).collect::<Result<_>>()?;
        Ok(Self { ids })
    }

    /// Encodes the blob.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.ids.len() * 4);
        put_u32(&mut out, 12);
        put_u32(&mut out, u32::try_from(self.ids.len()).unwrap_or(u32::MAX));
        put_u32(&mut out, 4);
        for id in &self.ids {
            put_u32(&mut out, *id);
        }
        out
    }

    /// Whether the setting `id` is shown.
    pub fn contains(&self, id: u32) -> bool {
        self.ids.contains(&id)
    }
}

/// One material a sub tool uses, as a `BrushPatternImageArray` or `TextureImage` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialRef {
    /// Catalog path, e.g. `.:Paint001:<uuid>:data:material.layer` (installed materials) or
    /// `.:44:34:<uuid>:data:material_0.layer` (own materials and those inside a `.sut`).
    pub path: String,
    /// Bytes after the path: a type word, the display name and an install path (layout in the
    /// README); kept verbatim.
    pub rest: Vec<u8>,
}

impl MaterialRef {
    /// The material's UUID: the path component before `data`.
    pub fn uuid(&self) -> Option<&str> {
        let parts: Vec<&str> = self.path.split(':').collect();
        let data = parts.iter().rposition(|p| *p == "data")?;
        parts.get(data.checked_sub(1)?).copied().filter(|u| !u.is_empty())
    }

    /// The file name inside the material's `data` folder (`material.layer`, `material_0.layer`).
    pub fn file_name(&self) -> Option<&str> {
        self.path.rsplit(':').next().filter(|f| !f.is_empty())
    }

    /// The display name stored after the path (`i32` type, `i32` byte length, UTF-16 LE name).
    pub fn name(&self) -> Option<String> {
        let mut r = Reader::new(&self.rest, REFS_WHAT);
        let _kind = r.u32_be().ok()?;
        let len = r.len_be().ok()?;
        let bytes = r.take(len).ok()?;
        utf16le(bytes).ok()
    }
}

/// A material reference list: `i32` 8, `i32` count, then per entry `i32` entry length (including
/// itself), `i32` path byte length, the path as UTF-16 LE and the rest of the entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialRefs {
    /// The entries in file order.
    pub entries: Vec<MaterialRef>,
}

impl MaterialRefs {
    /// Decodes the blob.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, REFS_WHAT);
        let header = r.len_be()?;
        if header < 8 {
            return malformed(REFS_WHAT, format!("header length {header}"));
        }
        let count = r.len_be()?;
        r.take(header - 8)?;
        if count > MAX_ENTRIES {
            return malformed(REFS_WHAT, format!("{count} entries"));
        }
        let mut entries = Vec::with_capacity(count.min(r.remaining() / 8));
        for _ in 0..count {
            let entry_len = r.len_be()?;
            if entry_len < 8 {
                return malformed(REFS_WHAT, format!("entry length {entry_len}"));
            }
            let mut e = Reader::new(r.take(entry_len - 4)?, REFS_WHAT);
            let path_len = e.len_be()?;
            let path = utf16le(e.take(path_len)?)?;
            entries.push(MaterialRef { path, rest: e.take(e.remaining())?.to_vec() });
        }
        if !r.is_empty() {
            return malformed(REFS_WHAT, format!("{} bytes after the last entry", r.remaining()));
        }
        Ok(Self { entries })
    }

    /// Encodes the blob.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        put_u32(&mut out, 8);
        put_len(&mut out, self.entries.len(), "material reference count")?;
        for entry in &self.entries {
            let path: Vec<u8> = entry.path.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let entry_len = 8usize.saturating_add(path.len()).saturating_add(entry.rest.len());
            put_len(&mut out, entry_len, "material reference length")?;
            put_len(&mut out, path.len(), "material path length")?;
            out.extend_from_slice(&path);
            out.extend_from_slice(&entry.rest);
        }
        Ok(out)
    }
}

/// Strict UTF-16 LE: an odd length or an unpaired surrogate is an error, so a decoded path always
/// encodes back to the same bytes. The raw blob stays in the variant row either way.
fn utf16le(bytes: &[u8]) -> Result<String> {
    if bytes.len() % 2 != 0 {
        return malformed(REFS_WHAT, "odd UTF-16 byte length");
    }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16(&units).or_else(|_| malformed(REFS_WHAT, "invalid UTF-16"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, name: &str) -> MaterialRef {
        let mut rest = Vec::new();
        put_u32(&mut rest, 3);
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        put_u32(&mut rest, name.len() as u32);
        rest.extend_from_slice(&name);
        rest.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        MaterialRef { path: path.into(), rest }
    }

    #[test]
    fn material_refs_round_trip_and_expose_uuid_and_name() {
        let refs = MaterialRefs {
            entries: vec![
                entry(".:Paint004:ad4b9d9fd6-4a4d-97b7-3cda-684a94dfba:data:material.layer", "Noise brush"),
                entry(".:45:36:58c44b1365-0944-b1af-d798-2ab384d329:data:material_0.layer", "Paper"),
            ],
        };
        let bytes = refs.encode().unwrap();
        let back = MaterialRefs::decode(&bytes).unwrap();
        assert_eq!(back, refs);
        assert_eq!(back.entries[0].uuid(), Some("ad4b9d9fd6-4a4d-97b7-3cda-684a94dfba"));
        assert_eq!(back.entries[1].uuid(), Some("58c44b1365-0944-b1af-d798-2ab384d329"));
        assert_eq!(back.entries[1].file_name(), Some("material_0.layer"));
        assert_eq!(back.entries[0].name().as_deref(), Some("Noise brush"));
        for cut in 0..bytes.len() {
            assert!(MaterialRefs::decode(&bytes[..cut]).is_err(), "cut {cut}");
        }
    }

    #[test]
    fn show_params_round_trip_and_reject_bad_counts() {
        let p = ShowParams { ids: vec![2000, 2010, 5400] };
        let bytes = p.encode();
        assert_eq!(ShowParams::decode(&bytes).unwrap(), p);
        assert!(p.contains(2010));
        for cut in 0..bytes.len() {
            assert!(ShowParams::decode(&bytes[..cut]).is_err(), "cut {cut}");
        }
        let mut bad = bytes.clone();
        bad[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(ShowParams::decode(&bad).is_err());
    }
}
