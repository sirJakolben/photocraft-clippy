//! `MaterialFile.FileData`: the tar archive of one embedded material.
//!
//! Clip Studio Paint writes ustar entries `catalog.zip`, `info.zip`, `data/material_0.layer`,
//! `thumbnail/thumbnail.png` and `icedata/layerData.xml`, mode 0777, owner `compress`.

use std::io::Read;

use crate::model::Material;
use crate::{Error, Limits, Result, check_limit, malformed};

const WHAT: &str = "material archive";
const MAX_ENTRIES: usize = 4096;

/// One file of a material archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Path inside the archive (`data/material_0.layer`).
    pub path: String,
    /// File contents.
    pub data: Vec<u8>,
}

/// The unpacked files of one embedded material.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialArchive {
    /// Entries in archive order; directories are skipped.
    pub entries: Vec<ArchiveEntry>,
    /// Modification time written into new archives (seconds since 1970).
    pub mtime: u64,
}

impl MaterialArchive {
    /// Unpacks a tar archive. Entry sizes and the total are capped by `limits`; only regular files
    /// are kept.
    pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Self> {
        let mut archive = tar::Archive::new(bytes);
        let mut out = Self::default();
        let mut total = 0u64;
        for entry in archive.entries().map_err(|e| tar_error(&e))? {
            let mut entry = entry.map_err(|e| tar_error(&e))?;
            if out.entries.len() >= MAX_ENTRIES {
                return malformed(WHAT, "too many entries");
            }
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let size = entry.header().size().map_err(|e| tar_error(&e))?;
            check_limit("archive entry size", size, limits.max_entry_bytes)?;
            total = total.saturating_add(size);
            check_limit("archive size", total, limits.max_file_bytes)?;
            let path = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
            if let Ok(t) = entry.header().mtime() {
                out.mtime = out.mtime.max(t);
            }
            let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(bytes.len()));
            entry.by_ref().take(size).read_to_end(&mut data)?;
            if data.len() as u64 != size {
                return malformed(WHAT, format!("{path} is truncated"));
            }
            out.entries.push(ArchiveEntry { path, data });
        }
        Ok(out)
    }

    /// Packs the entries the way Clip Studio Paint does (ustar, mode 0777, owner `compress`).
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut builder = tar::Builder::new(Vec::new());
        for entry in &self.entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(entry.data.len() as u64);
            header.set_mode(0o777);
            header.set_mtime(self.mtime);
            header.set_uid(0);
            header.set_gid(0);
            header.set_username("compress").map_err(|e| tar_error(&e))?;
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, &entry.path, entry.data.as_slice()).map_err(|e| tar_error(&e))?;
        }
        builder.into_inner().map_err(|e| tar_error(&e))
    }

    /// The entry at `path`.
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.path == path).map(|e| e.data.as_slice())
    }

    /// The brush tip image: the first `data/*.layer` entry.
    pub fn tip_layer(&self) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.path.starts_with("data/") && e.path.ends_with(".layer")).map(|e| e.data.as_slice())
    }
}

impl Material {
    /// Unpacks `FileData`.
    pub fn archive(&self, limits: &Limits) -> Result<MaterialArchive> {
        let data = self.file_data().ok_or_else(|| Error::Malformed { what: WHAT, reason: "no FileData".into() })?;
        MaterialArchive::decode(data, limits)
    }
}

fn tar_error(e: &std::io::Error) -> Error {
    Error::Malformed { what: WHAT, reason: e.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_round_trip_and_truncations_fail() {
        let archive = MaterialArchive {
            entries: vec![
                ArchiveEntry { path: "catalog.zip".into(), data: vec![1; 700] },
                ArchiveEntry { path: "data/material_0.layer".into(), data: vec![2; 1500] },
            ],
            mtime: 1_714_857_431,
        };
        let bytes = archive.encode().unwrap();
        let back = MaterialArchive::decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(back, archive);
        assert_eq!(back.tip_layer().map(<[u8]>::len), Some(1500));
        for cut in (1..bytes.len()).step_by(97) {
            if let Ok(a) = MaterialArchive::decode(&bytes[..cut], &Limits::default()) {
                assert!(a.entries.len() < 2 || a.entries.iter().all(|e| !e.data.is_empty()));
            }
        }
        let small = Limits { max_entry_bytes: 1000, ..Limits::default() };
        assert!(matches!(MaterialArchive::decode(&bytes, &small), Err(Error::Limit { .. })));
    }
}
