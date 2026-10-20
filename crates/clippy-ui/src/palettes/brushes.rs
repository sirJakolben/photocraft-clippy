//! Edit › Brushes (I2, I2a): in Clippy mode PhotoCraft's Edit › Toolbar… gives way to a Brushes
//! submenu with Import Brushes…, Export Brushes… and Save Brushes As…; Photoshop mode keeps
//! Toolbar…. The menu ids aren't engine commands: they ask for files or folders, then run
//! `clippy.stock.import`, `.export` and `.saveAs`. With the file or folder in their params
//! (`{"file"}`, `{"to"}`) they run at once, for agents.
//!
//! - Import (I3): `.sut` files go into the group the Sub Tool palette shows (or the one the Add
//!   Sub Tool link belongs to); a `.todb` becomes tools and groups as in Clip Studio Paint, with
//!   the brush tips from its material folder (found beside the database where Clip Studio Paint
//!   keeps it, else asked for); an `.abr` becomes a new group of the brush tool.
//! - Export (I4): a copy of the whole stock in a chosen folder; from a tool, group or sub tool's
//!   context menu (I6) a copy of just that.
//! - Save Brushes As (I5): copies the stock to a chosen folder and works there from then on; the
//!   active sub tool is selected again at its place in the copy.

use std::path::{Path, PathBuf};

use photocraft_clippy_stock::EntryPath;
use photocraft_engine::clippy_cmds::{STOCK_EXPORT, STOCK_IMPORT, STOCK_SAVE_AS};
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::{FileDialogAnswer, FileDialogRequest, PhotocraftApp};
use serde_json::{Value, json};

/// Edit › Brushes › Import Brushes….
pub const IMPORT: &str = "clippy.brushes.import";
/// Edit › Brushes › Export Brushes….
pub const EXPORT: &str = "clippy.brushes.export";
/// Edit › Brushes › Save Brushes As….
pub const SAVE_AS: &str = "clippy.brushes.saveAs";
/// PhotoCraft's Edit › Toolbar…, which the submenu replaces in Clippy mode.
const TOOLBAR: &str = "edit.toolbar";
/// The file types Import Brushes… takes.
const EXTENSIONS: [&str; 3] = ["sut", "todb", "abr"];

/// Edit › Brushes in place of Edit › Toolbar…, in Clippy mode.
pub(crate) fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    if !crate::mode::active(app) {
        return;
    }
    let Some(at) = items.iter().position(|i| i.id == TOOLBAR) else { return };
    let path = vec!["Edit".to_string(), "Brushes".to_string()];
    let item =
        |id: &str, label: &str| MenuItem { id: id.into(), label: label.into(), path: path.clone(), shortcut: None, enabled: true, checked: None, color: None };
    items.splice(at..=at, [item(IMPORT, "Import Brushes…"), item(EXPORT, "Export Brushes…"), item(SAVE_AS, "Save Brushes As…")]);
}

/// The Brushes menu ids.
pub(crate) fn invoke(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(match id {
        IMPORT => match text("file") {
            Some(file) => {
                let group = text("group").and_then(|g| EntryPath::parse(&g).ok());
                import_file(app, Path::new(&file), group, text("materials").map(PathBuf::from))
            }
            None => import_dialog(app, None),
        },
        EXPORT => match text("to") {
            Some(to) => export(app, text("path").and_then(|p| EntryPath::parse(&p).ok()), &to),
            None => export_dialog(app, None),
        },
        SAVE_AS => match text("to") {
            Some(to) => save_as(app, &to),
            None => save_as_dialog(app),
        },
        _ => return None,
    })
}

/// Ask for `.sut`, `.todb` or `.abr` files and import them; `.sut` files go into `group` (else the
/// group the Sub Tool palette shows).
pub fn import_dialog(app: &mut PhotocraftApp, group: Option<EntryPath>) -> Result<Value, String> {
    let request = FileDialogRequest::Open { multiple: true, initial_dir: None, extensions: Some(EXTENSIONS.iter().map(|e| (*e).to_string()).collect()) };
    app.ask_file(request, move |app, answer| match answer {
        FileDialogAnswer::Paths(paths) => {
            let mut last = Ok(Value::Null);
            for p in paths {
                last = import_file(app, Path::new(&p), group.clone(), None);
                if last.is_err() {
                    break;
                }
            }
            last
        }
        _ => Err(tl!("Sub tools can only be imported in the desktop app.").into()),
    })
}

/// Import one file (see the module docs).
fn import_file(app: &mut PhotocraftApp, file: &Path, group: Option<EntryPath>, materials: Option<PathBuf>) -> Result<Value, String> {
    let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let params = match ext.as_str() {
        "sut" => {
            let group = group.or_else(|| shown_group(app)).ok_or_else(|| tl!("Choose a sub tool group first (or create one).").to_string())?;
            json!({"file": file.to_string_lossy(), "group": group.to_string()})
        }
        "todb" => match materials.or_else(|| material_folder(file)) {
            Some(m) => json!({"file": file.to_string_lossy(), "materials": m.to_string_lossy()}),
            None => {
                // The brush tips are in Clip Studio Paint's material folder: ask for it.
                let file = file.to_owned();
                app.ui.status = tl!("Choose Clip Studio Paint's material folder (CLIPStudioCommon/Material) for the brush tips.").into();
                return app.ask_folder(move |app, folder| import_file(app, &file, None, Some(PathBuf::from(folder))));
            }
        },
        "abr" => json!({"file": file.to_string_lossy()}),
        _ => return Err(format!("{name}: {}", tl!("only .sut, .todb and .abr files can be imported."))),
    };
    let report = app.run(STOCK_IMPORT, params)?;
    app.ui.status = format!("{} {name}", tl!("Imported:"));
    Ok(report)
}

/// The group the Sub Tool palette shows.
fn shown_group(app: &mut PhotocraftApp) -> Option<EntryPath> {
    let tool = super::shown_tool(app)?;
    let group = super::shown_group(app, &tool)?;
    Some(EntryPath::Group(tool, group))
}

/// Clip Studio Paint's material folder for a tool database at its usual place:
/// `…/CELSYS/CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb` → `…/CELSYS/CLIPStudioCommon/Material`.
fn material_folder(todb: &Path) -> Option<PathBuf> {
    let celsys = todb.parent()?.parent()?.parent()?;
    let folder = celsys.join("CLIPStudioCommon").join("Material");
    folder.is_dir().then_some(folder)
}

/// Ask for a folder and export a copy of the stock, or of `path`, into it.
pub fn export_dialog(app: &mut PhotocraftApp, path: Option<EntryPath>) -> Result<Value, String> {
    app.ask_folder(move |app, folder| export(app, path, &folder))
}

fn export(app: &mut PhotocraftApp, path: Option<EntryPath>, to: &str) -> Result<Value, String> {
    let mut params = json!({"to": to});
    if let Some(p) = path {
        params["path"] = json!(p.to_string());
    }
    let made = app.run(STOCK_EXPORT, params)?;
    let at = made.get("path").and_then(Value::as_str).unwrap_or(to).to_owned();
    app.ui.status = format!("{} {at}", tl!("Brushes exported to"));
    Ok(made)
}

fn save_as_dialog(app: &mut PhotocraftApp) -> Result<Value, String> {
    app.ask_folder(|app, folder| save_as(app, &folder))
}

/// Save Brushes As: the stock is copied and used at `to` from now on.
fn save_as(app: &mut PhotocraftApp, to: &str) -> Result<Value, String> {
    let made = app.run(STOCK_SAVE_AS, json!({"to": to}))?;
    let at = made.get("stockDir").and_then(Value::as_str).unwrap_or(to).to_owned();
    app.ui.status = format!("{} {at}", tl!("The brushes are now saved in"));
    Ok(made)
}
