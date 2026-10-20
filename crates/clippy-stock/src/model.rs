//! The stock as loaded: tools → groups → sub tools in display order, each with its metadata.
//!
//! Loading never fails because of what is inside the stock folder (P10): unknown files and hidden
//! entries are ignored, unreadable metadata falls back to the defaults, a broken `.sut` becomes an
//! entry with a `warning`, and each such case is listed in [`Stock::warnings`] too. Only an
//! unreadable stock folder is an error.

use std::path::PathBuf;

use serde::Serialize;

use crate::names::EntryPath;

/// The loaded stock.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stock {
    /// The stock folder.
    pub root: PathBuf,
    /// Tools in display order.
    pub tools: Vec<ToolEntry>,
    /// Everything loading had to skip or fall back on, one line each.
    pub warnings: Vec<String>,
}

/// A tool folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEntry {
    /// Folder name.
    pub dir: String,
    /// Display name (metadata `name`, else the folder name).
    pub name: String,
    /// Icon id; `None` is the default icon.
    pub icon: Option<String>,
    /// Shortcut.
    pub shortcut: Option<String>,
    /// Groups in display order.
    pub groups: Vec<GroupEntry>,
    /// Why the metadata could not be read, if it couldn't.
    pub warning: Option<String>,
}

/// A sub tool group folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupEntry {
    /// Folder name.
    pub dir: String,
    /// Display name.
    pub name: String,
    /// Icon id; `None` is the default icon.
    pub icon: Option<String>,
    /// Shortcut.
    pub shortcut: Option<String>,
    /// Sub tools in display order.
    pub subtools: Vec<SubToolEntry>,
    /// Why the metadata could not be read, if it couldn't.
    pub warning: Option<String>,
}

/// A sub tool file.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubToolEntry {
    /// File name (`G-Pen.sut`).
    pub file: String,
    /// Display name: the file name without `.sut`.
    pub name: String,
    /// Shortcut (group metadata `shortcuts`).
    pub shortcut: Option<String>,
    /// The node UUID inside the file, hex; empty when the file is broken.
    pub uuid: String,
    /// The name stored in the file (`NodeName`); the display name follows the file name.
    pub node_name: String,
    /// `NodeInputOp` and `NodeOutputOp`: what kind of tool the sub tool is (`None` when the file
    /// is broken or lacks them).
    pub input_op: Option<i64>,
    pub output_op: Option<i64>,
    /// Why the file can't be used (P10); `None` for a good sub tool.
    pub warning: Option<String>,
}

impl Stock {
    /// The tool with folder name `dir`.
    pub fn tool(&self, dir: &str) -> Option<&ToolEntry> {
        self.tools.iter().find(|t| t.dir == dir)
    }

    /// The entry at `path`, as (tool, group, sub tool) indices.
    pub fn find(&self, path: &EntryPath) -> Option<(usize, Option<usize>, Option<usize>)> {
        let ti = |t: &str| self.tools.iter().position(|x| x.dir == t);
        match path {
            EntryPath::Tool(t) => Some((ti(t)?, None, None)),
            EntryPath::Group(t, g) => {
                let i = ti(t)?;
                let j = self.tools.get(i)?.groups.iter().position(|x| x.dir == *g)?;
                Some((i, Some(j), None))
            }
            EntryPath::SubTool(t, g, s) => {
                let i = ti(t)?;
                let group = self.tools.get(i)?;
                let j = group.groups.iter().position(|x| x.dir == *g)?;
                let file = crate::names::sut_file(s);
                let k = group.groups.get(j)?.subtools.iter().position(|x| x.file == file)?;
                Some((i, Some(j), Some(k)))
            }
        }
    }

    /// Whether `path` names an entry of this stock.
    pub fn contains(&self, path: &EntryPath) -> bool {
        self.find(path).is_some()
    }

    /// Every sub tool with its stock path.
    pub fn subtools(&self) -> impl Iterator<Item = (EntryPath, &SubToolEntry)> {
        self.tools.iter().flat_map(|t| {
            t.groups.iter().flat_map(move |g| {
                g.subtools
                    .iter()
                    .map(move |s| (EntryPath::SubTool(t.dir.clone(), g.dir.clone(), crate::names::sut_stem(&s.file).unwrap_or(&s.name).to_owned()), s))
            })
        })
    }

    /// Number of groups.
    pub fn group_count(&self) -> usize {
        self.tools.iter().map(|t| t.groups.len()).sum()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod load {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use photocraft_clippy_sut::{Limits, Sut};

    use super::{GroupEntry, Stock, SubToolEntry, ToolEntry};
    use crate::fsutil::{self, hex};
    use crate::names::{self, META_FILE};
    use crate::{Error, MAX_SUT_BYTES, Result, meta};

    /// What loading a `.sut` found, kept per file so a reload only reads changed files.
    #[derive(Clone, Debug)]
    struct Parsed {
        stamp: (u64, Option<SystemTime>),
        uuid: String,
        node_name: String,
        ops: (Option<i64>, Option<i64>),
        warning: Option<String>,
    }

    /// Sub tool files read before, by path and size and modification time.
    #[derive(Clone, Debug, Default)]
    pub struct LoadCache {
        files: HashMap<PathBuf, Parsed>,
    }

    impl LoadCache {
        /// Forgets files that are not in `stock` any more.
        fn retain(&mut self, keep: &std::collections::HashSet<PathBuf>) {
            self.files.retain(|p, _| keep.contains(p));
        }
    }

    impl Stock {
        /// Loads the stock folder `root`.
        pub fn load(root: &Path) -> Result<Stock> {
            Self::load_cached(root, &mut LoadCache::default())
        }

        /// [`Stock::load`] that reads only the `.sut` files changed since `cache` saw them.
        pub fn load_cached(root: &Path, cache: &mut LoadCache) -> Result<Stock> {
            let meta = std::fs::metadata(root).map_err(|e| Error::io(root, e))?;
            if !meta.is_dir() {
                return Err(Error::Invalid(format!("{} is not a folder", root.display())));
            }
            let mut stock = Stock { root: root.to_owned(), ..Default::default() };
            let mut seen = std::collections::HashSet::new();
            let (root_meta, warning) = meta::read(root);
            stock.warnings.extend(warning);
            let dirs = folders(root, &mut stock.warnings)?;
            for dir in root_meta.ordered(dirs) {
                let path = root.join(&dir);
                let (m, warning) = meta::read(&path);
                stock.warnings.extend(warning.clone());
                let mut tool = ToolEntry { name: m.display_name(&dir), icon: m.icon.clone(), shortcut: m.shortcut.clone(), dir, groups: Vec::new(), warning };
                let groups = folders(&path, &mut stock.warnings).unwrap_or_else(|e| {
                    stock.warnings.push(e.to_string());
                    Vec::new()
                });
                for gdir in m.ordered(groups) {
                    let gpath = path.join(&gdir);
                    tool.groups.push(load_group(&gpath, gdir, cache, &mut seen, &mut stock.warnings));
                }
                stock.tools.push(tool);
            }
            cache.retain(&seen);
            Ok(stock)
        }
    }

    fn load_group(path: &Path, dir: String, cache: &mut LoadCache, seen: &mut std::collections::HashSet<PathBuf>, warnings: &mut Vec<String>) -> GroupEntry {
        let (m, warning) = meta::read(path);
        warnings.extend(warning.clone());
        let mut group = GroupEntry { name: m.display_name(&dir), icon: m.icon.clone(), shortcut: m.shortcut.clone(), dir, subtools: Vec::new(), warning };
        let (entries, truncated) = match fsutil::list_dir(path) {
            Ok(x) => x,
            Err(e) => {
                warnings.push(e.to_string());
                return group;
            }
        };
        if truncated {
            warnings.push(format!("{}: more than {} entries, the rest are not shown", path.display(), crate::MAX_FOLDER_ENTRIES));
        }
        let files: Vec<String> = entries
            .into_iter()
            .filter(|(name, _, is_dir, _)| !is_dir && !name.starts_with('.') && names::sut_stem(name).is_some_and(|s| names::validate(s).is_ok()))
            .map(|(name, _, _, _)| name)
            .collect();
        for file in m.ordered(files) {
            let fpath = path.join(&file);
            seen.insert(fpath.clone());
            let parsed = parse(&fpath, cache);
            if let Some(w) = &parsed.warning {
                warnings.push(format!("{}: {w}", fpath.display()));
            }
            group.subtools.push(SubToolEntry {
                name: names::sut_stem(&file).unwrap_or(&file).to_owned(),
                shortcut: m.shortcuts.get(&file).cloned(),
                uuid: parsed.uuid,
                node_name: parsed.node_name,
                input_op: parsed.ops.0,
                output_op: parsed.ops.1,
                warning: parsed.warning,
                file,
            });
        }
        group
    }

    /// The subfolders of `dir` Clippy shows: real folders (not symbolic links), not hidden, with
    /// valid names.
    fn folders(dir: &Path, warnings: &mut Vec<String>) -> Result<Vec<String>> {
        let (entries, truncated) = fsutil::list_dir(dir)?;
        if truncated {
            warnings.push(format!("{}: more than {} entries, the rest are not shown", dir.display(), crate::MAX_FOLDER_ENTRIES));
        }
        let mut out = Vec::new();
        for (name, path, is_dir, is_link) in entries {
            if name.starts_with('.') || name == META_FILE {
                continue;
            }
            if is_link {
                warnings.push(format!("{}: symbolic links are not followed", path.display()));
            } else if is_dir {
                if names::validate(&name).is_ok() {
                    out.push(name);
                } else {
                    warnings.push(format!("{}: the folder name can't be used and is skipped", path.display()));
                }
            }
        }
        Ok(out)
    }

    fn parse(path: &Path, cache: &mut LoadCache) -> Parsed {
        let stamp = std::fs::metadata(path).map(|m| (m.len(), m.modified().ok())).unwrap_or((0, None));
        if let Some(hit) = cache.files.get(path)
            && hit.stamp == stamp
            && stamp.1.is_some()
        {
            return hit.clone();
        }
        let parsed = match fsutil::read_capped(path, MAX_SUT_BYTES).and_then(|b| Sut::from_bytes(&b, &Limits::default()).map_err(|e| Error::sut(path, e))) {
            Ok(sut) => Parsed {
                stamp,
                uuid: hex(sut.subtool.uuid()),
                node_name: sut.subtool.name().to_owned(),
                ops: (sut.subtool.input_op(), sut.subtool.output_op()),
                warning: None,
            },
            Err(Error::Sut { source, .. }) => {
                Parsed { stamp, uuid: String::new(), node_name: String::new(), ops: (None, None), warning: Some(format!("not a readable sub tool: {source}")) }
            }
            Err(e) => Parsed { stamp, uuid: String::new(), node_name: String::new(), ops: (None, None), warning: Some(format!("can't be read: {e}")) },
        };
        cache.files.insert(path.to_owned(), parsed.clone());
        parsed
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use load::LoadCache;
