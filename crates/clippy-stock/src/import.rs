//! Import (I1, I3): a `.sut` into a group, a whole Clip Studio Paint tool database (`.todb`) as
//! tools and groups, and PhotoCraft brush presets (from an `.abr`, read by the engine through
//! `photocraft_io::abr_map`) as a new group of the brush tool.
//!
//! A `.todb` is only read, never written (P8). Its sub tools become `.sut` files laid out like the
//! database, each with the brush tips and textures it uses embedded from Clip Studio Paint's
//! material folder (see [`MaterialIndex`]). The tools are built in a hidden folder inside the stock
//! and then moved into place one by one, so the stock never shows half a tool.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use photocraft_clippy_sut::{ArchiveEntry, Cell, Limits, Material, MaterialArchive, MaterialRef, Row, Sut, Todb, Value, known};
use photocraft_paint::BrushPreset;
use serde::Serialize;

use crate::fsutil::{self, name_taken};
use crate::meta::{self, FolderMeta};
use crate::names::{self, EntryPath};
use crate::ops::{self, child_names};
use crate::template::{self, TOOL_KIND_COLUMNS};
use crate::{Error, MAX_SUT_BYTES, Result, check_limit};

/// Largest `.todb` read.
pub const MAX_TODB_BYTES: u64 = 512 << 20;
/// Deepest folder level searched for materials.
pub const MAX_MATERIAL_DEPTH: usize = 12;
/// Most files and folders looked at in the material folder.
pub const MAX_MATERIAL_ENTRIES: u64 = 500_000;
/// Largest material folder packed into one sub tool.
pub const MAX_MATERIAL_BYTES: u64 = 256 << 20;
/// Tool names Clippy takes for "the brush tool" when an `.abr` names none (case-insensitive).
pub const BRUSH_TOOL_NAMES: [&str; 4] = ["Brush", "Pinsel", "ブラシ", "Brushes"];

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Tools created.
    pub tools: Vec<String>,
    /// Groups created, as `Tool/Group`.
    pub groups: Vec<String>,
    /// Sub tools written, as stock paths.
    pub subtools: Vec<String>,
    /// Materials embedded into sub tools.
    pub materials: usize,
    /// Materials a sub tool uses that the material folder does not have (catalog paths).
    pub missing_materials: Vec<String>,
    /// What else went wrong, one line each; the rest was imported.
    pub warnings: Vec<String>,
}

/// Imports the `.sut` at `file` into the group `tool/group` as its last entry, named after the
/// sub tool (numbered on a clash). The file is copied byte for byte; one Clippy can't read is
/// refused.
pub fn import_sut(root: &Path, file: &Path, tool: &str, group: &str) -> Result<EntryPath> {
    let target = EntryPath::Group(tool.to_owned(), group.to_owned());
    let dir = target.to_path(root);
    if !dir.is_dir() {
        return Err(Error::NotFound(format!("group {target}")));
    }
    let bytes = fsutil::read_capped(file, MAX_SUT_BYTES)?;
    let sut = Sut::from_bytes(&bytes, &Limits::default()).map_err(|e| Error::sut(file, e))?;
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Sub tool");
    let name = names::unique(&names::sanitize(sut.subtool.name(), stem), |n| name_taken(&dir, &names::sut_file(n)))?;
    let path = target.child(&name).ok_or_else(|| Error::Invalid("no group".into()))?;
    let dest = path.to_path(root);
    ops::append(&dir, &path.file_name(), true)?;
    fsutil::write_atomic(&dest, &bytes)?;
    Ok(path)
}

/// Imports a whole tool database: every tool with its groups and sub tools, in Clip Studio
/// Paint's order, names, icons and shortcut keys. Tools whose name exists get a number. Sub tools
/// get the materials they use from `materials` (Clip Studio Paint's material folder, searched by
/// material UUID); without it, or when one isn't there, it is listed in the report and the sub
/// tool refers to it by path as before.
pub fn import_todb(root: &Path, file: &Path, materials: Option<&Path>) -> Result<ImportReport> {
    let bytes = fsutil::read_capped(file, MAX_TODB_BYTES)?;
    let todb = Todb::from_bytes(&bytes, &Limits::default()).map_err(|e| Error::sut(file, e))?;
    let index = match materials {
        Some(dir) => MaterialIndex::scan(dir)?,
        None => MaterialIndex::default(),
    };
    let mut report = ImportReport::default();
    let staging = root.join(format!(".import-{}-{}", std::process::id(), fsutil::hex(&fsutil::new_uuid()[..4])));
    crate::own::note(&staging);
    fs::create_dir(&staging).map_err(|e| Error::io(&staging, e))?;
    let result = build_todb(root, &staging, &todb, &index, &mut report);
    let _ = fs::remove_dir_all(&staging);
    result.map(|()| report)
}

fn build_todb(root: &Path, staging: &Path, todb: &Todb, index: &MaterialIndex, report: &mut ImportReport) -> Result<()> {
    let mut placed: Vec<(PathBuf, String)> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut embedded: HashMap<String, Option<Material>> = HashMap::new();
    for (ti, tool) in todb.tree.tools.iter().enumerate() {
        let base = names::sanitize(tool.name(), &format!("Tool {}", ti + 1));
        let tname = names::unique(&base, |n| name_taken(root, n) || taken.contains(&n.to_lowercase()))?;
        taken.insert(tname.to_lowercase());
        let tdir = staging.join(&tname);
        fs::create_dir(&tdir).map_err(|e| Error::io(&tdir, e))?;
        let mut tmeta = node_meta(tool.name(), &tname, tool.icon(), tool.icon_color(), tool.shortcut());
        for (gi, group) in tool.groups.iter().enumerate() {
            let gname = names::unique(&names::sanitize(group.name(), &format!("Group {}", gi + 1)), |n| name_taken(&tdir, n))?;
            let gdir = tdir.join(&gname);
            fs::create_dir(&gdir).map_err(|e| Error::io(&gdir, e))?;
            let mut gmeta = node_meta(group.name(), &gname, group.icon(), group.icon_color(), group.shortcut());
            for (si, sub) in group.subtools.iter().enumerate() {
                let sname = names::unique(&names::sanitize(sub.name(), &format!("Sub tool {}", si + 1)), |n| name_taken(&gdir, &names::sut_file(n)))?;
                let mut sut = Sut::from_subtool(sub.clone(), &todb.info);
                embed_materials(&mut sut, index, &mut embedded, report);
                let file = names::sut_file(&sname);
                let fpath = gdir.join(&file);
                let written = sut.to_bytes().map_err(|e| Error::sut(&fpath, e)).and_then(|b| fsutil::write_atomic(&fpath, &b));
                match written {
                    Ok(()) => {
                        if let Some(code) = sub.shortcut() {
                            gmeta.extra.entry("cspShortcuts").or_insert_with(|| serde_json::json!({}));
                            if let Some(map) = gmeta.extra.get_mut("cspShortcuts").and_then(|v| v.as_object_mut()) {
                                map.insert(file.clone(), code.into());
                            }
                        }
                        gmeta.order.push(file);
                        report.subtools.push(format!("{tname}/{gname}/{sname}"));
                    }
                    Err(e) => report.warnings.push(format!("{}/{}/{}: not imported: {e}", tool.name(), group.name(), sub.name())),
                }
            }
            meta::write(&gdir, &gmeta)?;
            tmeta.order.push(gname.clone());
            report.groups.push(format!("{tname}/{gname}"));
        }
        meta::write(&tdir, &tmeta)?;
        placed.push((tdir, tname.clone()));
        report.tools.push(tname);
    }
    report.missing_materials = embedded.iter().filter(|(_, m)| m.is_none()).map(|(k, _)| k.clone()).collect();
    report.missing_materials.sort();
    // Into place: each tool moves in one rename, then the stock's order lists them last.
    for (dir, name) in &placed {
        let dest = root.join(name);
        crate::own::note_move(dir, &dest);
        fs::rename(dir, &dest).map_err(|e| Error::io(dir, e))?;
        ops::append(root, name, false)?;
    }
    Ok(())
}

/// Metadata for a tool or group from Clip Studio Paint: its exact name when the folder name had
/// to differ, its icon, and its shortcut key code (Clip Studio Paint's own numbering, kept for
/// the shortcut mapping).
fn node_meta(name: &str, folder: &str, icon: Option<i64>, color: Option<i64>, shortcut: Option<i64>) -> FolderMeta {
    let mut m = FolderMeta::default();
    let shown = meta::clean_text(name);
    if shown != folder && !shown.trim().is_empty() {
        m.name = Some(shown);
    }
    m.icon = icon.map(|i| format!("csp:{i}"));
    if let Some(c) = color {
        m.extra.insert("cspIconColor".into(), c.into());
    }
    if let Some(k) = shortcut {
        m.extra.insert("cspShortcut".into(), k.into());
    }
    m
}

/// Embeds into `sut` the materials its settings refer to that `index` has and the file doesn't
/// embed yet. Returns how many were embedded and the catalog paths of those not found.
pub fn embed(sut: &mut Sut, index: &MaterialIndex) -> (usize, Vec<String>) {
    let mut cache = HashMap::new();
    let mut report = ImportReport::default();
    embed_materials(sut, index, &mut cache, &mut report);
    let mut missing: Vec<String> = cache.into_iter().filter(|(_, m)| m.is_none()).map(|(k, _)| k).collect();
    missing.sort();
    (report.materials, missing)
}

/// Embeds the materials the sub tool's settings refer to, each once per sub tool.
fn embed_materials(sut: &mut Sut, index: &MaterialIndex, cache: &mut HashMap<String, Option<Material>>, report: &mut ImportReport) {
    let mut refs: Vec<MaterialRef> = Vec::new();
    for v in [&sut.subtool.variant, &sut.subtool.init_variant].into_iter().flatten() {
        for r in v.all_material_refs() {
            if !refs.iter().any(|x| x.path == r.path) {
                refs.push(r);
            }
        }
    }
    for r in refs {
        let catalog = catalog_path(&r.path);
        if sut.subtool.materials.iter().any(|m| m.catalog_path() == Some(catalog.as_str())) {
            continue;
        }
        let material = cache.entry(r.path.clone()).or_insert_with(|| index.material(&r).ok()).clone();
        if let Some(m) = material {
            sut.subtool.materials.push(m);
            report.materials += 1;
        }
    }
}

/// The catalog path of a material reference: the path without `:data:<file>`.
fn catalog_path(path: &str) -> String {
    match path.rfind(":data:") {
        Some(i) => path.get(..i).unwrap_or(path).to_owned(),
        None => path.to_owned(),
    }
}

/// The brush tips and textures in Clip Studio Paint's material folder, by UUID.
///
/// A material reference names its material by a catalog path such as
/// `.:Paint001:<uuid>:data:material.layer`. The index finds the `.layer` file of that name below
/// a folder named by the UUID (any depth below the material folder), and the material is that
/// folder: everything in it is packed the way a `.sut` embeds a material.
#[derive(Clone, Debug, Default)]
pub struct MaterialIndex {
    /// Folder name (lower case) → `.layer` files below folders of that name.
    by_folder: HashMap<String, Vec<PathBuf>>,
}

impl MaterialIndex {
    /// Indexes `dir` (bounded in depth and entries; symbolic links are not followed).
    pub fn scan(dir: &Path) -> Result<Self> {
        if !dir.is_dir() {
            return Err(Error::NotFound(format!("material folder {}", dir.display())));
        }
        let mut index = MaterialIndex::default();
        let mut count = 0;
        index.walk(dir, &mut Vec::new(), 0, &mut count)?;
        Ok(index)
    }

    fn walk(&mut self, dir: &Path, folders: &mut Vec<String>, depth: usize, count: &mut u64) -> Result<()> {
        if depth > MAX_MATERIAL_DEPTH {
            return Ok(());
        }
        let Ok((entries, _)) = fsutil::list_dir(dir) else { return Ok(()) };
        for (name, path, is_dir, is_link) in entries {
            *count += 1;
            check_limit("entries in the material folder", *count, MAX_MATERIAL_ENTRIES)?;
            if is_link {
                continue;
            }
            if is_dir {
                folders.push(name.to_lowercase());
                self.walk(&path, folders, depth + 1, count)?;
                folders.pop();
            } else if name.to_ascii_lowercase().ends_with(".layer") {
                for f in folders.iter() {
                    self.by_folder.entry(f.clone()).or_default().push(path.clone());
                }
            }
        }
        Ok(())
    }

    /// Whether nothing was indexed.
    pub fn is_empty(&self) -> bool {
        self.by_folder.is_empty()
    }

    /// The `.layer` file for a reference.
    pub fn find(&self, r: &MaterialRef) -> Option<&Path> {
        let uuid = r.uuid()?.to_lowercase();
        let file = r.file_name()?;
        self.by_folder.get(&uuid)?.iter().find(|p| p.file_name().and_then(|n| n.to_str()) == Some(file)).map(PathBuf::as_path)
    }

    /// The material for a reference as a `MaterialFile` row of a `.sut`.
    pub fn material(&self, r: &MaterialRef) -> Result<Material> {
        let layer = self.find(r).ok_or_else(|| Error::NotFound(format!("material {}", r.path)))?;
        // The material folder: the one above `data/`, else the tip's own folder.
        let parent = layer.parent().ok_or_else(|| Error::NotFound(r.path.clone()))?;
        let folder = if parent.file_name().is_some_and(|n| n == "data") { parent.parent().unwrap_or(parent) } else { parent };
        let mut entries = Vec::new();
        let mut total = 0u64;
        pack(folder, "", 0, &mut entries, &mut total)?;
        if !entries.iter().any(|e| e.path.ends_with(".layer")) {
            return Err(Error::NotFound(format!("material {}", r.path)));
        }
        let mtime = fs::metadata(layer).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let archive = MaterialArchive { entries, mtime };
        let data = archive.encode().map_err(|e| Error::sut(layer, e))?;
        let cells = [
            ("InstallFolder", Value::Integer(0)),
            ("OriginalPath", Value::Text(r.path.clone())),
            ("OldMaterial", Value::Integer(0)),
            ("FileData", Value::Blob(data)),
            ("CatalogPath", Value::Text(catalog_path(&r.path))),
            ("MaterialUuid", Value::Text(String::new())),
        ];
        let row = Row::from_cells(
            cells
                .into_iter()
                .map(|(name, value)| Cell {
                    name: name.into(),
                    decl_type: known::MATERIAL_FILE_COLUMNS.iter().find(|(n, _)| *n == name).map_or("", |(_, t)| t).into(),
                    value,
                })
                .collect(),
        );
        Ok(Material { row })
    }
}

fn pack(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<ArchiveEntry>, total: &mut u64) -> Result<()> {
    if depth > 4 {
        return Ok(());
    }
    let (entries, _) = fsutil::list_dir(dir)?;
    let mut entries: Vec<_> = entries.into_iter().filter(|(n, _, _, link)| !link && !n.starts_with('.')).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path, is_dir, _) in entries {
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if is_dir {
            pack(&path, &rel, depth + 1, out, total)?;
        } else {
            let data = fsutil::read_capped(&path, MAX_MATERIAL_BYTES)?;
            *total = total.saturating_add(data.len() as u64);
            check_limit("material size", *total, MAX_MATERIAL_BYTES)?;
            out.push(ArchiveEntry { path: rel, data });
        }
    }
    Ok(())
}

/// The tool `.abr` brushes go into: `tool` if given (created if missing), else the first tool
/// whose folder or display name is one of [`BRUSH_TOOL_NAMES`], else a new tool "Brush".
pub fn brush_tool(root: &Path, tool: Option<&str>) -> Result<String> {
    if let Some(t) = tool {
        names::validate(t)?;
        if !root.join(t).is_dir() {
            ops::create_tool(root, t)?;
        }
        return Ok(t.to_owned());
    }
    for dir in child_names(root, false) {
        let shown = meta::read(&root.join(&dir)).0.display_name(&dir);
        if BRUSH_TOOL_NAMES.iter().any(|b| b.eq_ignore_ascii_case(&dir) || b.eq_ignore_ascii_case(&shown)) {
            return Ok(dir);
        }
    }
    Ok(ops::create_tool(root, "Brush")?.name().to_owned())
}

/// Imports brush presets (I3: an `.abr` file's brushes) as a new group named `group` (numbered on
/// a clash) in the brush tool ([`brush_tool`]). Each preset becomes a `.sut` holding its size,
/// opacity and tip in Clip Studio Paint's columns and the whole preset for PhotoCraft
/// ([`crate::PRESET_COLUMN`]). Sub tools of the tool lend the new ones their tool kind.
pub fn import_presets(root: &Path, tool: Option<&str>, group: &str, presets: &[BrushPreset]) -> Result<(EntryPath, ImportReport)> {
    let tool = brush_tool(root, tool)?;
    let kind = tool_kind(&root.join(&tool));
    let gpath = ops::create_group(root, &tool, &names::sanitize(group, "Brushes"))?;
    let gdir = gpath.to_path(root);
    let mut report = ImportReport { groups: vec![gpath.to_string()], ..Default::default() };
    let mut order = Vec::new();
    for (i, preset) in presets.iter().enumerate() {
        let name = names::unique(&names::sanitize(&preset.name, &format!("Brush {}", i + 1)), |n| name_taken(&gdir, &names::sut_file(n)))?;
        let file = names::sut_file(&name);
        let path = gdir.join(&file);
        let written = template::preset_sut(preset, fsutil::new_uuid(), &kind)
            .and_then(|s| s.to_bytes().map_err(|e| Error::sut(&path, e)))
            .and_then(|b| fsutil::write_atomic(&path, &b));
        match written {
            Ok(()) => {
                order.push(file);
                report.subtools.push(format!("{gpath}/{name}"));
            }
            Err(e) => report.warnings.push(format!("{}: not imported: {e}", preset.name)),
        }
    }
    meta::update(&gdir, |m| m.order = order)?;
    Ok((gpath, report))
}

/// The tool kind columns of the first readable sub tool of a tool folder.
fn tool_kind(tool_dir: &Path) -> Vec<(String, Value)> {
    for group in child_names(tool_dir, false) {
        let gdir = tool_dir.join(group);
        for file in child_names(&gdir, true) {
            let Ok(bytes) = fsutil::read_capped(&gdir.join(&file), MAX_SUT_BYTES) else { continue };
            let Ok(sut) = Sut::from_bytes(&bytes, &Limits::default()) else { continue };
            let kind: Vec<_> =
                TOOL_KIND_COLUMNS.iter().filter_map(|c| sut.subtool.node.get(c).filter(|v| !v.is_null()).map(|v| ((*c).to_owned(), v.clone()))).collect();
            if !kind.is_empty() {
                return kind;
            }
        }
    }
    Vec::new()
}
