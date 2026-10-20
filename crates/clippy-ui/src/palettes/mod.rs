//! Clippy's tool palettes (M9): Tool ([`tools`], S8, S17), Sub Tool ([`subtools`], S2–S7, S15,
//! S16), Tool Property ([`property`], S9–S12) and Edit › Brushes ([`brushes`], I2–I6).
//!
//! They show the sub tool stock (`photocraft_clippy_stock`) as it is on disk: [`View`] opens it in
//! the background, keeps it up to date while a file manager or Clippy changes it, and remembers
//! which group each tool shows and which sub tool each tool and group used last. Every change goes
//! through engine commands (`clippy.stock.*`, `clippy.subtool.*`); the palettes keep only what
//! they show.
//!
//! The active sub tool is the preference `clippy.subtool` (a `.sut` path). When it moves inside
//! the stock (renamed, dragged to another group, moved in a file manager) the preference and the
//! data flow follow it; when it is deleted nothing is selected.

pub mod brushes;
mod controls;
mod dialog;
mod dnd;
mod preview;
pub mod property;
mod quick;
pub mod subtools;
pub mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use photocraft_clippy_stock::{EntryPath, GroupEntry, Stock, SubToolEntry, ToolEntry};
use photocraft_engine::clippy_cmds::{self, SUBTOOL_SELECT};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::state::Tool;
use serde_json::{Value, json};

pub use dialog::dialogs;
pub use dnd::ghost as drag_ghost;

/// What a drag in the palettes carries (`egui::DragAndDrop` payload).
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    SubTool(EntryPath),
    Group(EntryPath),
    /// A tool, by folder name.
    Tool(String),
}

/// The palettes' view of the stock: the loaded stock and what the user last chose in it.
#[derive(Default)]
pub struct View {
    #[cfg(not(target_arch = "wasm32"))]
    live: Option<photocraft_clippy_stock::live::LiveStock>,
    #[cfg(not(target_arch = "wasm32"))]
    opening: Option<std::sync::mpsc::Receiver<Result<photocraft_clippy_stock::live::LiveStock, String>>>,
    /// The stock as last loaded, shared with the palettes while they draw.
    stock: Option<Arc<Stock>>,
    /// The folder `live` (or `opening`) is for.
    root: Option<PathBuf>,
    /// Why the stock couldn't be opened, and the own-change revision it failed at (a change by
    /// Clippy, such as an import creating the folder, tries again).
    error: Option<(String, u64)>,
    /// The tool the Sub Tool palette shows.
    shown_tool: Option<String>,
    /// The group each tool shows, by tool folder.
    groups: HashMap<String, String>,
    /// The sub tool each tool (`"Tool"`) and group (`"Tool/Group"`) used last.
    last: HashMap<String, EntryPath>,
    /// The active sub tool as last seen, to notice selections made elsewhere.
    seen: Option<EntryPath>,
    /// Select this sub tool once the stock at a new folder is open (Save Brushes As).
    reselect: Option<EntryPath>,
    pub(crate) previews: preview::Cache,
}

fn view(app: &mut PhotocraftApp) -> &mut View {
    &mut crate::subtool::state(app).view
}

/// The stock as last loaded; `None` until it is open (or when it can't be).
pub fn stock(app: &mut PhotocraftApp) -> Option<Arc<Stock>> {
    view(app).stock.clone()
}

/// Why the stock isn't shown, if it isn't.
pub fn stock_error(app: &mut PhotocraftApp) -> Option<String> {
    view(app).error.as_ref().map(|(e, _)| e.clone())
}

/// The stock folder the palettes show.
pub fn root(app: &mut PhotocraftApp) -> Option<PathBuf> {
    view(app).stock.as_ref().map(|s| s.root.clone())
}

/// The active sub tool as a stock path, when it lies in the stock.
pub fn active_entry(app: &mut PhotocraftApp) -> Option<EntryPath> {
    let path = app.session.prefs().clippy.subtool.clone();
    let root = root(app)?;
    if path.is_empty() {
        return None;
    }
    EntryPath::from_disk(&root, std::path::Path::new(&path))
}

/// The tool the Sub Tool palette shows.
pub fn shown_tool(app: &mut PhotocraftApp) -> Option<String> {
    let stock = stock(app)?;
    let v = view(app);
    v.shown_tool.clone().filter(|t| stock.tool(t).is_some()).or_else(|| stock.tools.first().map(|t| t.dir.clone()))
}

/// The group the Sub Tool palette shows for `tool`.
pub fn shown_group(app: &mut PhotocraftApp, tool: &str) -> Option<String> {
    let stock = stock(app)?;
    let entry = stock.tool(tool)?;
    let chosen = view(app).groups.get(tool).cloned();
    chosen.filter(|g| entry.groups.iter().any(|x| &x.dir == g)).or_else(|| entry.groups.first().map(|g| g.dir.clone()))
}

/// Once per frame in Clippy mode: open the stock (again, when its folder changed), apply what
/// changed on disk, and follow the active sub tool when it moved.
pub fn frame(app: &mut PhotocraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    desktop::poll(app);
    let active = active_entry(app);
    let v = view(app);
    if active.is_some() && active != v.seen {
        // Selected elsewhere (a command, a restart): its tool and group are the ones shown.
        if let Some(EntryPath::SubTool(t, g, _)) = &active {
            v.shown_tool = Some(t.clone());
            v.groups.insert(t.clone(), g.clone());
        }
        if let Some(p) = &active {
            remember(v, p);
        }
    }
    v.seen = active;
}

fn remember(v: &mut View, path: &EntryPath) {
    if let EntryPath::SubTool(t, g, _) = path {
        v.last.insert(t.clone(), path.clone());
        v.last.insert(format!("{t}/{g}"), path.clone());
    }
}

/// Make the sub tool at `path` the active one (`clippy.subtool.select`).
pub fn select(app: &mut PhotocraftApp, path: &EntryPath) -> Result<Value, String> {
    let root = root(app).ok_or_else(|| tl!("The sub tool stock isn't open.").to_string())?;
    let disk = path.to_path(&root);
    let result = app.run(SUBTOOL_SELECT, json!({"path": disk.to_string_lossy()}));
    if result.is_ok() {
        let v = view(app);
        remember(v, path);
        if let EntryPath::SubTool(t, g, _) = path {
            v.shown_tool = Some(t.clone());
            v.groups.insert(t.clone(), g.clone());
        }
        v.seen = Some(path.clone());
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(live) = v.live.as_mut() {
            live.select(Some(path.clone()));
        }
    }
    result
}

/// Show `tool` and select the sub tool it used last (else the first of its shown group).
pub fn select_tool(app: &mut PhotocraftApp, tool: &str) -> Result<Value, String> {
    let Some(stock) = stock(app) else { return Ok(Value::Null) };
    view(app).shown_tool = Some(tool.to_owned());
    let last = view(app).last.get(tool).cloned().filter(|p| stock.contains(p));
    let target = last.or_else(|| {
        let group = shown_group(app, tool)?;
        first_subtool(&stock, tool, &group)
    });
    match target {
        Some(p) => select(app, &p),
        None => Ok(Value::Null),
    }
}

/// Show `group` of `tool` and select the sub tool it used last (else its first).
pub fn select_group(app: &mut PhotocraftApp, tool: &str, group: &str) -> Result<Value, String> {
    let Some(stock) = stock(app) else { return Ok(Value::Null) };
    let v = view(app);
    v.shown_tool = Some(tool.to_owned());
    v.groups.insert(tool.to_owned(), group.to_owned());
    let last = v.last.get(&format!("{tool}/{group}")).cloned().filter(|p| stock.contains(p));
    match last.or_else(|| first_subtool(&stock, tool, group)) {
        Some(p) => select(app, &p),
        None => Ok(Value::Null),
    }
}

fn first_subtool(stock: &Stock, tool: &str, group: &str) -> Option<EntryPath> {
    let g = stock.tool(tool)?.groups.iter().find(|g| g.dir == group)?;
    let s = g.subtools.iter().find(|s| s.warning.is_none()).or(g.subtools.first())?;
    Some(EntryPath::SubTool(tool.to_owned(), group.to_owned(), photocraft_clippy_stock::names::sut_stem(&s.file).unwrap_or(&s.name).to_owned()))
}

/// The stock path of a sub tool entry of `group` in `tool`.
pub fn subtool_path(tool: &str, group: &str, s: &SubToolEntry) -> EntryPath {
    EntryPath::SubTool(tool.to_owned(), group.to_owned(), photocraft_clippy_stock::names::sut_stem(&s.file).unwrap_or(&s.name).to_owned())
}

/// The PhotoCraft tool a sub tool entry maps to, by its kind (`NodeInputOp`/`NodeOutputOp`).
pub fn entry_tool(s: &SubToolEntry) -> Option<Tool> {
    crate::mapping::tool::choose(s.input_op, s.output_op, &s.node_name, None).tool()
}

/// Icon of a group (S15): its own when it is one of [`GROUP_ICONS`], else the icon of the
/// PhotoCraft tool its first mappable sub tool maps to; a dashed circle when PhotoCraft has a tool
/// for none of them, a folder when it is empty.
pub fn group_icon(g: &GroupEntry) -> &'static str {
    if let Some(own) = g.icon.as_deref().and_then(icon_name) {
        return own;
    }
    match g.subtools.iter().find_map(entry_tool) {
        Some(tool) => photocraft_ui_egui::icons::tool_icon(tool),
        None if g.subtools.is_empty() => "folder",
        None => "circle-dashed",
    }
}

/// Icon of a tool in the Tool palette (S17): the icon of the group it shows.
pub fn tool_icon(t: &ToolEntry, shown_group: Option<&str>) -> &'static str {
    if let Some(own) = t.icon.as_deref().and_then(icon_name) {
        return own;
    }
    let group = shown_group.and_then(|g| t.groups.iter().find(|x| x.dir == g)).or(t.groups.first());
    group.map_or("folder", group_icon)
}

/// Icons a group can be given (S15), from PhotoCraft's set; M10 brings Clippy's own.
pub const GROUP_ICONS: &[&str] = &[
    "pencil",
    "pen-tool",
    "pen-freeform",
    "brush",
    "eraser",
    "droplet",
    "blend",
    "paint-bucket",
    "gradient",
    "stamp",
    "sparkles",
    "star",
    "flame",
    "cloud",
    "sun",
    "moon",
    "wand",
    "wand-sparkles",
    "lasso",
    "square-dashed",
    "ellipse-dashed",
    "move",
    "hand",
    "zoom-in",
    "pipette",
    "type",
    "ruler",
    "spline",
    "slash",
    "rectangle-horizontal",
    "circle",
    "pentagon",
    "triangle",
    "diamond",
    "palette",
    "swatch-book",
    "scissors",
    "bandage",
    "layers",
    "image",
    "grid-3x3",
    "folder",
];

/// A stored icon id as one of the icons a group can be given; `csp:<n>` (Clip Studio Paint's
/// own, which Clippy doesn't ship) and unknown names are `None`.
fn icon_name(id: &str) -> Option<&'static str> {
    GROUP_ICONS.iter().copied().find(|n| *n == id)
}

/// Where the stock is (or should be), from the preferences and the app's settings folder.
fn wanted_root(app: &PhotocraftApp) -> Result<(PathBuf, bool), String> {
    clippy_cmds::stock_dir(&app.session).map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::mpsc::{self, TryRecvError};

    use photocraft_clippy_stock::live::LiveStock;
    use photocraft_clippy_stock::{EntryPath, ops, own};
    use photocraft_ui_egui::PhotocraftApp;

    use super::{View, view, wanted_root};

    /// Open the stock at its folder (in the background: a stock of a few hundred sub tools takes
    /// a moment to read), apply changes, follow the active sub tool.
    pub(super) fn poll(app: &mut PhotocraftApp) {
        let wanted = wanted_root(app);
        let v = view(app);
        match wanted {
            Err(e) => {
                close(v);
                v.error = Some((e, own::revision()));
            }
            Ok((root, default)) => {
                if v.root.as_deref() != Some(root.as_path()) {
                    // Another folder (Save Brushes As, the preference): the active sub tool is
                    // looked for at the same place in it.
                    if v.reselect.is_none() {
                        v.reselect = v.live.as_ref().and_then(|l| l.selected().cloned());
                    }
                    close(v);
                    open(v, root, default);
                } else if v.live.is_none() && v.opening.is_none() && v.error.as_ref().is_some_and(|(_, rev)| *rev != own::revision()) {
                    open(v, root, default);
                }
            }
        }
        receive(app);
        apply_changes(app);
    }

    fn close(v: &mut View) {
        v.live = None;
        v.opening = None;
        v.stock = None;
        v.root = None;
    }

    fn open(v: &mut View, root: PathBuf, default: bool) {
        let (tx, rx) = mpsc::channel();
        v.root = Some(root.clone());
        v.error = None;
        v.opening = Some(rx);
        let spawned = std::thread::Builder::new().name("clippy-stock".into()).spawn(move || {
            let opened = if default { ops::ensure_root(&root).map_err(|e| e.to_string()) } else { Ok(()) }
                .and_then(|()| LiveStock::open(&root).map_err(|e| e.to_string()));
            // Nobody waits any more when another folder was chosen meanwhile.
            let _ = tx.send(opened);
        });
        if let Err(e) = spawned {
            v.opening = None;
            v.error = Some((e.to_string(), own::revision()));
        }
    }

    fn receive(app: &mut PhotocraftApp) {
        let v = view(app);
        let Some(rx) = v.opening.as_ref() else { return };
        let opened = match rx.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(opened) => opened,
            Err(TryRecvError::Disconnected) => Err(tl!("Reading the sub tool stock stopped unexpectedly.").to_string()),
        };
        v.opening = None;
        match opened {
            Ok(mut live) => {
                let root = live.stock().root.clone();
                let selected = from_pref(&app.session.prefs().clippy.subtool, &root);
                let v = view(app);
                live.select(selected);
                v.stock = Some(Arc::new(live.stock().clone()));
                v.live = Some(live);
                if let Some(p) = v.reselect.take().filter(|p| v.stock.as_ref().is_some_and(|s| s.contains(p)))
                    && let Err(e) = super::select(app, &p)
                {
                    app.ui.status = e;
                }
            }
            Err(e) => {
                v.error = Some((format!("{} {e}", tl!("The sub tool stock couldn't be opened:")), own::revision()));
                v.reselect = None;
            }
        }
    }

    fn from_pref(pref: &str, root: &Path) -> Option<EntryPath> {
        (!pref.is_empty()).then(|| EntryPath::from_disk(root, Path::new(pref))).flatten()
    }

    /// Apply what changed on disk. A move of the active sub tool takes the preference and the
    /// data flow along; its deletion leaves nothing selected.
    fn apply_changes(app: &mut PhotocraftApp) {
        let pref = app.session.prefs().clippy.subtool.clone();
        let v = view(app);
        let Some(live) = v.live.as_mut() else { return };
        let root = live.stock().root.clone();
        // Selected elsewhere (a command): the live stock follows that one.
        let selected = from_pref(&pref, &root);
        if live.selected() != selected.as_ref() {
            live.select(selected);
        }
        let before = live.selected().cloned();
        match live.poll() {
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(e) => {
                app.ui.status = format!("{} {e}", tl!("The sub tool stock couldn't be read:"));
                return;
            }
        }
        let after = live.selected().cloned();
        v.stock = Some(Arc::new(live.stock().clone()));
        if before == after {
            return;
        }
        let new_path = after.as_ref().map(|p| p.to_path(&root).to_string_lossy().into_owned()).unwrap_or_default();
        crate::subtool::retarget(app, &new_path);
    }
}
