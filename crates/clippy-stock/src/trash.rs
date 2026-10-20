//! Deleting to the trash, never for good.
//!
//! Linux and the BSDs use the home trash of the freedesktop.org Trash specification 1.0
//! (`$XDG_DATA_HOME/Trash`, else `~/.local/share/Trash`): the entry moves to `files/`, and
//! `info/<name>.trashinfo` records where it came from and when, so file managers can restore it.
//! An entry on another file system is copied there and then removed. macOS and Windows use their
//! own trash through the `trash` crate.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Where deleted entries go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trash {
    /// The user's trash.
    System,
    /// A freedesktop.org trash folder at this path (`files/`, `info/`): tests, and systems where
    /// the home trash is elsewhere.
    Freedesktop(PathBuf),
}

/// Moves `path` (a file or folder) to `trash`.
pub fn delete(path: &Path, trash: &Trash) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Err(Error::NotFound(path.display().to_string()));
    }
    crate::own::note(path);
    match trash {
        Trash::Freedesktop(dir) => freedesktop(path, dir),
        Trash::System => system(path),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn system(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| Error::io(path, std::io::Error::other(e.to_string())))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system(path: &Path) -> Result<()> {
    let dir = home_trash().ok_or(Error::NotSupported("no home folder to find the trash in (set HOME or XDG_DATA_HOME)"))?;
    freedesktop(path, &dir)
}

/// `$XDG_DATA_HOME/Trash`, else `$HOME/.local/share/Trash`.
pub fn home_trash() -> Option<PathBuf> {
    let absolute = |v: std::ffi::OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").and_then(absolute) {
        return Some(data.join("Trash"));
    }
    std::env::var_os("HOME").and_then(absolute).map(|h| h.join(".local/share/Trash"))
}

fn freedesktop(path: &Path, trash: &Path) -> Result<()> {
    let files = trash.join("files");
    let info = trash.join("info");
    for dir in [&files, &info] {
        create_private_dir(dir)?;
    }
    let absolute = std::path::absolute(path).map_err(|e| Error::io(path, e))?;
    let base = absolute.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid(format!("{} has no file name", path.display())))?.to_owned();
    // Reserve a name: the .trashinfo file is created exclusively, as the specification asks.
    let mut reserved = None;
    for n in 1..10_000u32 {
        let name = if n == 1 { base.clone() } else { format!("{base}.{n}") };
        let info_path = info.join(format!("{name}.trashinfo"));
        if files.join(&name).exists() {
            continue;
        }
        match fs::OpenOptions::new().write(true).create_new(true).open(&info_path) {
            Ok(mut f) => {
                let text = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", encode_path(&absolute), deletion_date());
                if let Err(e) = f.write_all(text.as_bytes()).and_then(|()| f.sync_all()) {
                    let _ = fs::remove_file(&info_path);
                    return Err(Error::io(&info_path, e));
                }
                reserved = Some((name, info_path));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(Error::io(&info_path, e)),
        }
    }
    let (name, info_path) = reserved.ok_or_else(|| Error::Exists(format!("{base} in the trash, 10 000 times")))?;
    let target = files.join(&name);
    let moved = match fs::rename(path, &target) {
        Ok(()) => Ok(()),
        // Another file system: copy into the trash, then remove the original.
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => copy_then_remove(path, &target),
        Err(e) => Err(Error::io(path, e)),
    };
    if moved.is_err() {
        let _ = fs::remove_file(&info_path);
    }
    moved
}

fn copy_then_remove(path: &Path, target: &Path) -> Result<()> {
    if path.is_dir() {
        crate::fsutil::copy_tree(path, target, &|_, b| b)?;
        fs::remove_dir_all(path).map_err(|e| Error::io(path, e))
    } else {
        fs::copy(path, target).map_err(|e| Error::io(path, e))?;
        fs::remove_file(path).map_err(|e| Error::io(path, e))
    }
}

fn create_private_dir(dir: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir).map_err(|e| Error::io(dir, e))
}

/// The path, percent-encoded as the specification's `Path=` key wants (RFC 2396 escaping, `/`
/// kept).
fn encode_path(path: &Path) -> String {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"/-_.~!*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `YYYY-MM-DDThh:mm:ss` now. The specification asks for local time; the standard library knows
/// no time zones, so this is UTC (file managers show and restore the entry all the same).
fn deletion_date() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_move_to_the_trash_with_info_files() {
        let dir = tempfile::tempdir().unwrap();
        let trash = Trash::Freedesktop(dir.path().join("Trash"));
        let folder = dir.path().join("stock/Pen 1%");
        fs::create_dir_all(folder.join("Glatt")).unwrap();
        fs::write(folder.join("Glatt/a.sut"), b"x").unwrap();
        delete(&folder, &trash).unwrap();
        assert!(!folder.exists());
        assert_eq!(fs::read(dir.path().join("Trash/files/Pen 1%/Glatt/a.sut")).unwrap(), b"x");
        let info = fs::read_to_string(dir.path().join("Trash/info/Pen 1%.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(info.contains("/stock/Pen%201%25\n"), "{info}");
        assert!(info.contains("DeletionDate=20"), "{info}");
        // A second entry of the same name gets its own slot.
        fs::create_dir_all(&folder).unwrap();
        delete(&folder, &trash).unwrap();
        assert!(dir.path().join("Trash/files/Pen 1%.2").is_dir());
        assert!(dir.path().join("Trash/info/Pen 1%.2.trashinfo").is_file());
        assert!(matches!(delete(&folder, &trash), Err(Error::NotFound(_))));
    }

    #[test]
    fn the_date_is_a_civil_date() {
        let d = deletion_date();
        assert_eq!(d.len(), 19);
        assert_eq!(&d[4..5], "-");
        assert_eq!(&d[10..11], "T");
    }
}
