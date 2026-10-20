//! Changes to the stock: create, rename, delete, move, copy and order entries, and set their
//! metadata. Each takes the stock folder and stock paths ([`EntryPath`]); names are checked
//! ([`names::validate`]), a name clash on a move or copy gets a number ("G-Pen 2"), a copied sub
//! tool is an independent file with its own node UUID (P5), and deletions go to the trash.
//!
//! Every function notes its paths in [`crate::own`] before it touches them, so the watcher knows
//! the change as Clippy's own. Order: the target's metadata is written first, then the entry moves,
//! then the source's metadata is tidied; a stale name in an `order` list is harmless (ignored), so
//! a failure part-way never hides an entry.

use std::fs;
use std::path::{Path, PathBuf};

use photocraft_clippy_sut::{Limits, Sut};

use crate::fsutil::{self, name_taken};
use crate::meta::{self, FolderMeta};
use crate::names::{self, EntryPath, META_FILE};
use crate::trash::{self, Trash};
use crate::{Error, MAX_SUT_BYTES, Result};

/// The folder holding `path`'s entry: the stock for a tool.
pub fn parent_dir(root: &Path, path: &EntryPath) -> PathBuf {
    path.parent().map_or_else(|| root.to_owned(), |p| p.to_path(root))
}

/// The folder of `parent` (`None` = the stock itself).
fn folder_of(root: &Path, parent: Option<&EntryPath>) -> PathBuf {
    parent.map_or_else(|| root.to_owned(), |p| p.to_path(root))
}

/// Fails unless `dir` is a real folder (not a symbolic link).
fn require_dir(dir: &Path, what: &str) -> Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(Error::Invalid(format!("{what} {} is not a folder", dir.display()))),
        Err(_) => Err(Error::NotFound(format!("{what} {}", dir.display()))),
    }
}

/// Fails unless the entry `path` exists as what it is (folder or `.sut` file).
fn require_entry(root: &Path, path: &EntryPath) -> Result<PathBuf> {
    let disk = path.to_path(root);
    match path {
        EntryPath::SubTool(..) => match fs::metadata(&disk) {
            Ok(m) if m.is_file() => Ok(disk),
            Ok(_) => Err(Error::Invalid(format!("{} is not a file", disk.display()))),
            Err(_) => Err(Error::NotFound(format!("sub tool {path}"))),
        },
        _ => require_dir(&disk, path.kind()).map(|()| disk),
    }
}

/// The names of the entries in `dir` that the stock shows: folders, or `.sut` files when
/// `subtools`.
pub fn child_names(dir: &Path, subtools: bool) -> Vec<String> {
    let Ok((entries, _)) = fsutil::list_dir(dir) else { return Vec::new() };
    entries
        .into_iter()
        .filter(|(name, _, is_dir, is_link)| {
            if name.starts_with('.') || name == META_FILE {
                return false;
            }
            if subtools {
                !is_dir && names::sut_stem(name).is_some_and(|s| names::validate(s).is_ok())
            } else {
                *is_dir && !is_link && names::validate(name).is_ok()
            }
        })
        .map(|(name, ..)| name)
        .collect()
}

/// Puts `name` into `dir`'s order at `index` (end when `None` or past the end), writing out the
/// current display order so the place sticks; carries `shortcut` along.
fn place(dir: &Path, name: &str, index: Option<usize>, subtools: bool, shortcut: Option<String>) -> Result<()> {
    let others: Vec<String> = child_names(dir, subtools).into_iter().filter(|n| n != name).collect();
    meta::update(dir, |m| {
        let mut order = m.ordered(others);
        let at = index.unwrap_or(order.len()).min(order.len());
        order.insert(at, name.to_owned());
        m.order = order;
        if let Some(key) = shortcut {
            m.shortcuts.insert(name.to_owned(), key);
        }
    })
}

/// Puts `name` last in `dir`'s order (a new entry Clippy made).
pub(crate) fn append(dir: &Path, name: &str, subtools: bool) -> Result<()> {
    place(dir, name, None, subtools, None)
}

/// Rewrites a `.sut` in place through `f`; a file that can't be read is left as it is.
fn rewrite_sut(path: &Path, f: impl FnOnce(&mut Sut)) -> Result<()> {
    let Ok(bytes) = fsutil::read_capped(path, MAX_SUT_BYTES) else { return Ok(()) };
    let Ok(mut sut) = Sut::from_bytes(&bytes, &Limits::default()) else { return Ok(()) };
    f(&mut sut);
    let out = sut.to_bytes().map_err(|e| Error::sut(path, e))?;
    fsutil::write_atomic(path, &out)
}

/// `bytes` of a `.sut` with a fresh node UUID and, if given, a new name; other bytes unchanged.
pub(crate) fn fresh_copy(path: &Path, bytes: Vec<u8>, name: Option<&str>) -> Vec<u8> {
    if names::sut_stem(&path.file_name().and_then(|n| n.to_str()).map(str::to_owned).unwrap_or_default()).is_none() {
        return bytes;
    }
    let Ok(mut sut) = Sut::from_bytes(&bytes, &Limits::default()) else { return bytes };
    sut = Sut::from_subtool(
        {
            let mut s = sut.subtool;
            s.node.set("NodeUuid", photocraft_clippy_sut::Value::Blob(fsutil::new_uuid()));
            if let Some(name) = name {
                s.set_name(name);
            }
            s
        },
        &sut.info,
    );
    sut.to_bytes().unwrap_or(bytes)
}

/// Creates the stock folder if it is missing.
pub fn ensure_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root).map_err(|e| Error::io(root, e))?;
    require_dir(root, "stock folder")
}

/// Creates a tool folder named `name` (numbered on a clash), last in the tool order.
pub fn create_tool(root: &Path, name: &str) -> Result<EntryPath> {
    require_dir(root, "stock folder")?;
    create_folder(root, None, name)
}

/// Creates a group folder named `name` (numbered on a clash) in `tool`, last in its order.
pub fn create_group(root: &Path, tool: &str, name: &str) -> Result<EntryPath> {
    let parent = EntryPath::Tool(tool.to_owned());
    require_entry(root, &parent)?;
    create_folder(root, Some(&parent), name)
}

fn create_folder(root: &Path, parent: Option<&EntryPath>, name: &str) -> Result<EntryPath> {
    let dir = folder_of(root, parent);
    let name = names::unique(name, |n| name_taken(&dir, n))?;
    let path = dir.join(&name);
    crate::own::note(&path);
    fs::create_dir(&path).map_err(|e| Error::io(&path, e))?;
    place(&dir, &name, None, false, None)?;
    Ok(parent.and_then(|p| p.child(&name)).unwrap_or(EntryPath::Tool(name)))
}

/// Renames an entry. A sub tool's file is renamed and the name inside it (`NodeName`) follows; a
/// folder's display name becomes the folder name. Another entry of that name is an error.
pub fn rename(root: &Path, path: &EntryPath, new_name: &str) -> Result<EntryPath> {
    names::validate(new_name)?;
    let from = require_entry(root, path)?;
    let target = path.with_name(new_name);
    if target == *path {
        return Ok(target);
    }
    let dir = parent_dir(root, path);
    let to = target.to_path(root);
    // A clash is another entry with the name in any case; changing only the case is allowed.
    let same_entry = target.file_name().to_lowercase() == path.file_name().to_lowercase();
    if !same_entry && name_taken(&dir, &target.file_name()) {
        return Err(Error::Exists(format!("{} {target}", target.kind())));
    }
    crate::own::note_move(&from, &to);
    fs::rename(&from, &to).map_err(|e| Error::io(&from, e))?;
    match &target {
        EntryPath::SubTool(..) => rewrite_sut(&to, |sut| sut.subtool.set_name(new_name))?,
        _ => meta::update(&to, |m| m.name = None)?,
    }
    meta::update(&dir, |m| m.rename_entry(&path.file_name(), &target.file_name()))?;
    Ok(target)
}

/// Moves an entry to the trash and forgets it in its folder's metadata.
pub fn delete(root: &Path, path: &EntryPath, to: &Trash) -> Result<()> {
    let disk = require_entry(root, path)?;
    trash::delete(&disk, to)?;
    let dir = parent_dir(root, path);
    meta::update(&dir, |m| {
        m.remove_entry(&path.file_name());
    })
}

/// The folder an entry of this kind can go into: tools into the stock (`None`), groups into a
/// tool, sub tools into a group.
fn check_target(path: &EntryPath, to: Option<&EntryPath>) -> Result<()> {
    let ok = matches!(
        (path, to),
        (EntryPath::Tool(_), None) | (EntryPath::Group(..), Some(EntryPath::Tool(_))) | (EntryPath::SubTool(..), Some(EntryPath::Group(..)))
    );
    if ok {
        Ok(())
    } else {
        let to = to.map_or_else(|| "the stock".to_owned(), |t| format!("{} {t}", t.kind()));
        Err(Error::Invalid(format!("a {} can't go into {to}: tools go into the stock, groups into a tool, sub tools into a group", path.kind())))
    }
}

/// Moves an entry into the folder `to` at `index` (end when `None`); within its own folder this
/// reorders it. The moved file is unchanged; on a name clash it gets a number. Returns its new path.
pub fn move_entry(root: &Path, path: &EntryPath, to: Option<&EntryPath>, index: Option<usize>) -> Result<EntryPath> {
    check_target(path, to)?;
    let from = require_entry(root, path)?;
    if let Some(t) = to {
        require_entry(root, t)?;
    }
    let src_dir = parent_dir(root, path);
    let dst_dir = folder_of(root, to);
    let subtools = matches!(path, EntryPath::SubTool(..));
    if path.parent().as_ref() == to {
        place(&dst_dir, &path.file_name(), index, subtools, None)?;
        return Ok(path.clone());
    }
    let name = names::unique(path.name(), |n| name_taken(&dst_dir, &path.with_name(n).file_name()))?;
    let target = to.and_then(|t| t.child(&name)).ok_or_else(|| Error::Invalid("no target folder".into()))?;
    let dest = target.to_path(root);
    let shortcut = meta::read(&src_dir).0.shortcuts.get(&path.file_name()).cloned();
    place(&dst_dir, &target.file_name(), index, subtools, shortcut)?;
    crate::own::note_move(&from, &dest);
    fs::rename(&from, &dest).map_err(|e| Error::io(&from, e))?;
    if subtools && name != path.name() {
        rewrite_sut(&dest, |sut| sut.subtool.set_name(&name))?;
    }
    // The move happened; a metadata file that can't be tidied only keeps a stale name.
    let _ = meta::update(&src_dir, |m| {
        m.remove_entry(&path.file_name());
    });
    Ok(target)
}

/// Copies an entry into the folder `to` at `index` (end when `None`; `to` may be its own folder).
/// The copy is independent (P5): its own files, and every copied sub tool gets a fresh node UUID.
/// On a name clash it gets a number. Returns the copy's path.
pub fn copy_entry(root: &Path, path: &EntryPath, to: Option<&EntryPath>, index: Option<usize>) -> Result<EntryPath> {
    check_target(path, to)?;
    let from = require_entry(root, path)?;
    if let Some(t) = to {
        require_entry(root, t)?;
    }
    let dst_dir = folder_of(root, to);
    let name = names::unique(path.name(), |n| name_taken(&dst_dir, &path.with_name(n).file_name()))?;
    let target = match to {
        Some(t) => t.child(&name).ok_or_else(|| Error::Invalid("no target folder".into()))?,
        None => EntryPath::Tool(name.clone()),
    };
    let dest = target.to_path(root);
    let subtools = matches!(path, EntryPath::SubTool(..));
    let shortcut = None; // a shortcut belongs to one sub tool; the copy has none
    place(&dst_dir, &target.file_name(), index, subtools, shortcut)?;
    crate::own::note(&dest);
    if subtools {
        let bytes = fsutil::read_capped(&from, MAX_SUT_BYTES)?;
        let renamed = (name != path.name()).then_some(name.as_str());
        fsutil::write_atomic(&dest, &fresh_copy(&from, bytes, renamed))?;
    } else {
        fsutil::copy_tree(&from, &dest, &|p, b| fresh_copy(p, b, None))?;
        // The copy's display name is its folder name, not the original's.
        meta::update(&dest, |m| m.name = None)?;
    }
    Ok(target)
}

/// Sets the order of the entries in `parent` (`None` = the tools of the stock). `order` lists
/// entry names (`G-Pen.sut` or `G-Pen` for sub tools); names not in the folder are an error,
/// entries left out follow alphabetically. `None` clears the order: all alphabetical. Returns the
/// resulting display order.
pub fn sort(root: &Path, parent: Option<&EntryPath>, order: Option<&[String]>) -> Result<Vec<String>> {
    if matches!(parent, Some(EntryPath::SubTool(..))) {
        return Err(Error::Invalid("a sub tool has no entries to sort".into()));
    }
    let dir = folder_of(root, parent);
    match parent {
        Some(p) => {
            require_entry(root, p)?;
        }
        None => require_dir(root, "stock folder")?,
    }
    let subtools = matches!(parent, Some(EntryPath::Group(..)));
    let present = child_names(&dir, subtools);
    let mut wanted = Vec::new();
    for name in order.unwrap_or_default() {
        let file = if subtools && names::sut_stem(name).is_none() { names::sut_file(name) } else { name.clone() };
        if !present.contains(&file) {
            return Err(Error::NotFound(format!(
                "`{}` in {}",
                name.chars().take(80).collect::<String>(),
                parent.map_or_else(|| "the stock".to_owned(), ToString::to_string)
            )));
        }
        if !wanted.contains(&file) {
            wanted.push(file);
        }
    }
    meta::update(&dir, |m| m.order = wanted)?;
    Ok(meta::read(&dir).0.ordered(present))
}

/// A change to an entry's metadata; `None` leaves a field, `Some(None)` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetaChange {
    /// Display name of a tool or group (sub tools are renamed with [`rename`]).
    pub name: Option<Option<String>>,
    /// Icon id of a tool or group.
    pub icon: Option<Option<String>>,
    /// Shortcut of a tool, group or sub tool.
    pub shortcut: Option<Option<String>>,
}

/// Sets display name, icon or shortcut (P9).
pub fn set_meta(root: &Path, path: &EntryPath, change: &MetaChange) -> Result<FolderMeta> {
    let disk = require_entry(root, path)?;
    let clean = |v: &Option<String>| v.as_deref().map(meta::clean_text).filter(|t| !t.trim().is_empty());
    if let EntryPath::SubTool(..) = path {
        if change.name.is_some() || change.icon.is_some() {
            return Err(Error::Invalid("a sub tool's name is its file name (rename it) and its icon comes from its file".into()));
        }
        let dir = parent_dir(root, path);
        let file = path.file_name();
        return meta::update(&dir, |m| {
            if let Some(key) = &change.shortcut {
                match clean(key) {
                    Some(k) => m.shortcuts.insert(file.clone(), k),
                    None => m.shortcuts.remove(&file),
                };
            }
            m.clone()
        });
    }
    meta::update(&disk, |m| {
        if let Some(v) = &change.name {
            m.name = clean(v);
        }
        if let Some(v) = &change.icon {
            m.icon = clean(v);
        }
        if let Some(v) = &change.shortcut {
            m.shortcut = clean(v);
        }
        m.clone()
    })
}
