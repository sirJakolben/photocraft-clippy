//! Export (I4, I6) and "Save Brushes As" (I5): folder copies.
//!
//! Exports are verbatim copies, a backup or something to hand on: the same files, UUIDs and
//! metadata. "Save Brushes As" copies the whole stock to a new place; the caller then works there
//! (the engine sets `clippy.stockDir`) and the old place stays as it was.

use std::fs;
use std::path::{Path, PathBuf};

use crate::fsutil::{self, name_taken};
use crate::names::{self, EntryPath};
use crate::{Error, Result};

/// Copies the whole stock (`path` = `None`, I4) or one tool, group or sub tool (I6) into the
/// folder `dest`, as a new entry named like the original (numbered on a clash). Returns the
/// copy's path.
pub fn export(root: &Path, path: Option<&EntryPath>, dest: &Path) -> Result<PathBuf> {
    let src = path.map_or_else(|| root.to_owned(), |p| p.to_path(root));
    if fs::symlink_metadata(&src).is_err() {
        return Err(Error::NotFound(path.map_or_else(|| format!("stock folder {}", root.display()), |p| format!("{} {p}", p.kind()))));
    }
    if !dest.is_dir() {
        return Err(Error::NotFound(format!("folder {}", dest.display())));
    }
    if fsutil::is_within(dest, &src) {
        return Err(Error::Invalid(format!("{} is inside what is exported; choose a folder outside it", dest.display())));
    }
    let base = match path {
        Some(p) => p.name().to_owned(),
        None => names::sanitize(root.file_name().and_then(|n| n.to_str()).unwrap_or(""), "Clippy stock"),
    };
    let file_name = |n: &str| match path {
        Some(EntryPath::SubTool(..)) => names::sut_file(n),
        _ => n.to_owned(),
    };
    let name = names::unique(&base, |n| name_taken(dest, &file_name(n)))?;
    let target = dest.join(file_name(&name));
    if src.is_dir() {
        fsutil::copy_tree(&src, &target, &|_, b| b)?;
    } else {
        fs::copy(&src, &target).map_err(|e| Error::io(&target, e))?;
    }
    Ok(target)
}

/// "Save Brushes As" (I5): copies the whole stock to `dest`, which must be missing or an empty
/// folder, and returns it. The stock at `root` is not changed; a missing `root` (a stock never
/// used) gives an empty folder.
pub fn save_as(root: &Path, dest: &Path) -> Result<PathBuf> {
    if fsutil::is_within(dest, root) {
        return Err(Error::Invalid(format!("{} is inside the stock; choose a folder outside it", dest.display())));
    }
    match fs::read_dir(dest) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(Error::Exists(format!("{} has files in it; choose an empty or new folder", dest.display())));
            }
            fs::remove_dir(dest).map_err(|e| Error::io(dest, e))?;
        }
        Err(_) if dest.exists() => return Err(Error::Invalid(format!("{} is not a folder", dest.display()))),
        Err(_) => {}
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    if root.is_dir() {
        fsutil::copy_tree(root, dest, &|_, b| b)?;
    } else {
        fs::create_dir(dest).map_err(|e| Error::io(dest, e))?;
    }
    Ok(dest.to_owned())
}
