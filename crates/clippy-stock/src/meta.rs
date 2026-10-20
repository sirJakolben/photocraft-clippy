//! `_clippy.json`: what a folder name can't say (P9). One per stock, tool and group folder, every
//! key optional:
//!
//! ```json
//! {
//!   "name": "Pen (G)",              // display name; missing: the folder name
//!   "icon": "csp:110",              // icon id; missing: the default icon
//!   "shortcut": "P",                // shortcut of this tool or group
//!   "order": ["G-Pen.sut", "Fineliner.sut"],  // entries in this order, the rest after them, alphabetically
//!   "shortcuts": {"G-Pen.sut": "Shift+P"}     // shortcuts of the sub tools (group folders)
//! }
//! ```
//!
//! Entries are named as on disk (`<name>.sut` for sub tools, the folder name for tools and
//! groups). Keys Clippy doesn't know are kept when it writes the file. An unreadable file reads as
//! the defaults with a warning; before Clippy writes over it, it keeps it as `_clippy.json.broken`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Longest display name, icon id or shortcut kept, in characters.
pub const MAX_TEXT_CHARS: usize = 256;
/// Most `order` entries kept.
pub const MAX_ORDER: usize = crate::MAX_FOLDER_ENTRIES;

/// The metadata of one folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FolderMeta {
    /// Display name; `None` shows the folder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Icon id (`csp:<NodeIcon>` for icons from Clip Studio Paint); `None` is the default icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Shortcut of the folder's tool or group, in PhotoCraft's notation (`Shift+P`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// Entry names in display order; entries not listed follow alphabetically.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// Shortcuts of entries (the sub tools of a group), by entry name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub shortcuts: BTreeMap<String, String>,
    /// Keys this version doesn't know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FolderMeta {
    /// Reads the JSON text of a metadata file. Wrong types for known keys are an error; texts are
    /// cut to [`MAX_TEXT_CHARS`] and stripped of control characters, `order` to [`MAX_ORDER`].
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        if !value.is_object() {
            return Err("not a JSON object".into());
        }
        let mut meta: FolderMeta = serde_json::from_value(value).map_err(|e| e.to_string())?;
        for text in [&mut meta.name, &mut meta.icon, &mut meta.shortcut].into_iter().flatten() {
            *text = clean_text(text);
        }
        meta.order.truncate(MAX_ORDER);
        meta.shortcuts.retain(|k, _| k.len() <= 512);
        for v in meta.shortcuts.values_mut() {
            *v = clean_text(v);
        }
        Ok(meta)
    }

    /// The JSON text Clippy writes (pretty, so it reads well in an editor).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Whether nothing is set: the file can go.
    pub fn is_empty(&self) -> bool {
        *self == FolderMeta::default()
    }

    /// Display name: `name` if set and not blank, else `folder`.
    pub fn display_name(&self, folder: &str) -> String {
        self.name.as_deref().filter(|n| !n.trim().is_empty()).unwrap_or(folder).to_owned()
    }

    /// `names` (the entries on disk) in display order: those in `order` first, in that order,
    /// then the rest alphabetically (case-insensitive, then exact).
    pub fn ordered(&self, mut names: Vec<String>) -> Vec<String> {
        let mut out = Vec::with_capacity(names.len());
        for wanted in &self.order {
            if let Some(i) = names.iter().position(|n| n == wanted) {
                out.push(names.swap_remove(i));
            }
        }
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
        out.extend(names);
        out
    }

    /// Renames an entry in `order` and `shortcuts`.
    pub fn rename_entry(&mut self, from: &str, to: &str) {
        for name in &mut self.order {
            if name == from {
                *name = to.to_owned();
            }
        }
        if let Some(key) = self.shortcuts.remove(from) {
            self.shortcuts.insert(to.to_owned(), key);
        }
    }

    /// Forgets an entry; returns its shortcut.
    pub fn remove_entry(&mut self, name: &str) -> Option<String> {
        self.order.retain(|n| n != name);
        self.shortcuts.remove(name)
    }
}

/// `text` without control characters, at most [`MAX_TEXT_CHARS`] characters.
pub fn clean_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(MAX_TEXT_CHARS).collect()
}

#[cfg(not(target_arch = "wasm32"))]
mod disk {
    use std::path::Path;

    use super::FolderMeta;
    use crate::names::META_FILE;
    use crate::{Error, MAX_META_BYTES, Result, fsutil};

    /// The broken file kept before Clippy writes a new one over it.
    pub const BROKEN_FILE: &str = "_clippy.json.broken";

    /// Reads `dir/_clippy.json`: the defaults when it is missing; the defaults and a warning when
    /// it can't be read.
    pub fn read(dir: &Path) -> (FolderMeta, Option<String>) {
        let path = dir.join(META_FILE);
        let warn = |why: String| (FolderMeta::default(), Some(format!("{}: {why}; using folder names and alphabetical order", path.display())));
        let len = match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => return warn("not a file".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (FolderMeta::default(), None),
            Err(e) => return warn(e.to_string()),
        };
        if len > MAX_META_BYTES {
            return warn(format!("{len} bytes, more than {MAX_META_BYTES}"));
        }
        match std::fs::read(&path) {
            Ok(bytes) => match FolderMeta::from_json(&String::from_utf8_lossy(&bytes)) {
                Ok(meta) => (meta, None),
                Err(e) => warn(e),
            },
            Err(e) => warn(e.to_string()),
        }
    }

    /// Writes `dir/_clippy.json` atomically, or removes it when `meta` is empty. An unreadable
    /// file there is kept as [`BROKEN_FILE`] first.
    pub fn write(dir: &Path, meta: &FolderMeta) -> Result<()> {
        let path = dir.join(META_FILE);
        if let (_, Some(_)) = read(dir)
            && path.is_file()
        {
            let broken = dir.join(BROKEN_FILE);
            std::fs::rename(&path, &broken).map_err(|e| Error::io(&path, e))?;
            crate::own::note(&broken);
        }
        crate::own::note(&path);
        if meta.is_empty() {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(Error::io(&path, e)),
            };
        }
        fsutil::write_atomic(&path, meta.to_json().as_bytes())
    }

    /// Reads, changes and writes the metadata of `dir` (an unreadable file starts from the
    /// defaults). Nothing is written when `f` leaves it as it was.
    pub fn update<T>(dir: &Path, f: impl FnOnce(&mut FolderMeta) -> T) -> Result<T> {
        let (mut meta, warning) = read(dir);
        let before = meta.clone();
        let out = f(&mut meta);
        if meta != before || (warning.is_some() && !meta.is_empty()) {
            write(dir, &meta)?;
        }
        Ok(out)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use disk::{BROKEN_FILE, read, update, write};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_are_defaults_and_unknown_keys_survive() {
        let meta = FolderMeta::from_json(r#"{"name":"Pen","future":{"x":1},"order":["b.sut"]}"#).unwrap();
        assert_eq!(meta.name.as_deref(), Some("Pen"));
        assert_eq!(meta.icon, None);
        let back = FolderMeta::from_json(&meta.to_json()).unwrap();
        assert_eq!(back, meta);
        assert_eq!(back.extra["future"], serde_json::json!({"x": 1}));
        assert!(FolderMeta::from_json("{}").unwrap().is_empty());
    }

    #[test]
    fn bad_files_are_errors_not_panics() {
        for text in ["", "[]", "1", "null", "{", r#"{"name":1}"#, r#"{"order":"a"}"#, r#"{"shortcuts":[]}"#, "\u{0}"] {
            assert!(FolderMeta::from_json(text).is_err(), "{text:?}");
        }
        let long = format!(r#"{{"name":"{}\u0007"}}"#, "x".repeat(10_000));
        let meta = FolderMeta::from_json(&long).unwrap();
        assert_eq!(meta.name.unwrap().chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn order_lists_first_then_alphabetical() {
        let meta = FolderMeta { order: vec!["c".into(), "gone".into(), "a".into(), "c".into()], ..Default::default() };
        let names = ["b", "a", "C2", "c", "B"].map(String::from).to_vec();
        assert_eq!(meta.ordered(names.clone()), ["c", "a", "B", "b", "C2"]);
        assert_eq!(FolderMeta::default().ordered(names), ["a", "B", "b", "c", "C2"]);
    }

    #[test]
    fn display_names_fall_back_to_the_folder() {
        let mut meta = FolderMeta::default();
        assert_eq!(meta.display_name("Pen"), "Pen");
        meta.name = Some("  ".into());
        assert_eq!(meta.display_name("Pen"), "Pen");
        meta.name = Some("Stift".into());
        assert_eq!(meta.display_name("Pen"), "Stift");
    }
}
