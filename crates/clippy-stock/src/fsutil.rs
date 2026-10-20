//! Small file-system helpers: atomic writes, capped reads, folder copies, fresh node UUIDs.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Error, MAX_FOLDER_ENTRIES, Result, check_limit};

/// Deepest folder level a copy descends to (a stock has three).
pub const MAX_COPY_DEPTH: usize = 8;
/// Most files and folders one copy handles.
pub const MAX_COPY_ENTRIES: u64 = 200_000;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` through a hidden temporary file in the same folder (synced, then
/// renamed), so a crash or a full disk never leaves a half-written file under the final name.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid(format!("{} has no file name", path.display())))?;
    let tmp = path.with_file_name(format!(".{name}.tmp{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    crate::own::note(path);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| Error::io(path, e))
}

/// Reads a file of at most `limit` bytes.
pub fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!("{} is not a file", path.display())));
    }
    check_limit("file size", meta.len(), limit)?;
    fs::read(path).map_err(|e| Error::io(path, e))
}

/// Folder entries: (name, path, is a real folder, is a symbolic link).
pub type Listing = Vec<(String, PathBuf, bool, bool)>;

/// The entries of a folder (not following symbolic links), at most [`MAX_FOLDER_ENTRIES`]. Names
/// that aren't UTF-8 are skipped. The bool says whether entries were left out.
pub fn list_dir(dir: &Path) -> Result<(Listing, bool)> {
    let mut out = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let Ok(entry) = entry else { continue };
        if out.len() >= MAX_FOLDER_ENTRIES {
            truncated = true;
            break;
        }
        let Ok(name) = entry.file_name().into_string() else { continue };
        let Ok(kind) = entry.file_type() else { continue };
        out.push((name, entry.path(), kind.is_dir(), kind.is_symlink()));
    }
    Ok((out, truncated))
}

/// Whether `dir` holds an entry named `name`, compared case-insensitively (as on macOS and
/// Windows, so a stock stays valid when synced there).
pub fn name_taken(dir: &Path, name: &str) -> bool {
    let lower = name.to_lowercase();
    match fs::read_dir(dir) {
        Ok(entries) => entries.flatten().take(MAX_FOLDER_ENTRIES * 2).any(|e| e.file_name().to_str().is_some_and(|n| n.to_lowercase() == lower)),
        Err(_) => dir.join(name).exists(),
    }
}

/// Copies the folder `src` to `dst` (which must not exist): regular files and folders, no
/// symbolic links and no hidden entries, at most [`MAX_COPY_DEPTH`] levels and
/// [`MAX_COPY_ENTRIES`] entries. `map` may replace a file's bytes on the way (fresh UUIDs for
/// copied sub tools). A failed copy removes what it wrote.
pub fn copy_tree(src: &Path, dst: &Path, map: &dyn Fn(&Path, Vec<u8>) -> Vec<u8>) -> Result<()> {
    if dst.exists() {
        return Err(Error::Exists(dst.display().to_string()));
    }
    let mut count = 0;
    let result = copy_level(src, dst, 0, &mut count, map);
    if result.is_err() {
        let _ = fs::remove_dir_all(dst);
    }
    result
}

fn copy_level(src: &Path, dst: &Path, depth: usize, count: &mut u64, map: &dyn Fn(&Path, Vec<u8>) -> Vec<u8>) -> Result<()> {
    if depth > MAX_COPY_DEPTH {
        return Err(Error::Limit { what: "folder depth", value: depth as u64, limit: MAX_COPY_DEPTH as u64 });
    }
    fs::create_dir(dst).map_err(|e| Error::io(dst, e))?;
    crate::own::note(dst);
    let (entries, _) = list_dir(src)?;
    for (name, path, is_dir, is_link) in entries {
        if is_link || name.starts_with('.') {
            continue;
        }
        *count += 1;
        check_limit("entries to copy", *count, MAX_COPY_ENTRIES)?;
        let target = dst.join(&name);
        if is_dir {
            copy_level(&path, &target, depth + 1, count, map)?;
        } else {
            let bytes = fs::read(&path).map_err(|e| Error::io(&path, e))?;
            let bytes = map(&path, bytes);
            fs::write(&target, bytes).map_err(|e| Error::io(&target, e))?;
        }
    }
    Ok(())
}

/// A fresh 16-byte node UUID (random, version 4 layout). Not cryptographic; only unique.
pub fn new_uuid() -> Vec<u8> {
    use std::hash::{BuildHasher, Hasher};
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut out = Vec::with_capacity(16);
    for half in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(time);
        h.write_u64(n);
        h.write_u64(half);
        h.write_u32(std::process::id());
        out.extend_from_slice(&h.finish().to_le_bytes());
    }
    if let Some(b) = out.get_mut(6) {
        *b = (*b & 0x0f) | 0x40;
    }
    if let Some(b) = out.get_mut(8) {
        *b = (*b & 0x3f) | 0x80;
    }
    out
}

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `inner` is `outer` or inside it, after resolving both as far as they exist.
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let resolve = |p: &Path| -> PathBuf {
        // Resolve the longest existing prefix; the rest is appended as written.
        let mut existing = p.to_path_buf();
        let mut rest = Vec::new();
        while !existing.exists() {
            match (existing.file_name().map(std::ffi::OsStr::to_os_string), existing.parent()) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    existing = parent.to_path_buf();
                }
                _ => break,
            }
        }
        let mut out = fs::canonicalize(&existing).unwrap_or(existing);
        for name in rest.into_iter().rev() {
            out.push(name);
        }
        out
    };
    resolve(inner).starts_with(resolve(outer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_differ_and_have_the_version_bits() {
        let a = new_uuid();
        let b = new_uuid();
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
        assert_eq!(a[6] >> 4, 4);
    }

    #[test]
    fn copies_skip_links_and_hidden_files_and_clean_up_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), b"a").unwrap();
        fs::write(src.join(".hidden"), b"h").unwrap();
        fs::write(src.join("sub/b.txt"), b"b").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/", src.join("link")).unwrap();
        let dst = dir.path().join("dst");
        copy_tree(&src, &dst, &|_, b| b).unwrap();
        assert_eq!(fs::read(dst.join("sub/b.txt")).unwrap(), b"b");
        assert!(!dst.join(".hidden").exists() && !dst.join("link").exists());
        assert!(matches!(copy_tree(&src, &dst, &|_, b| b), Err(Error::Exists(_))));
        assert!(is_within(&dst.join("sub/new/x"), &dst));
        assert!(!is_within(&src, &dst));
        assert!(is_within(&dst.join("sub/../sub"), &dst));
    }
}
