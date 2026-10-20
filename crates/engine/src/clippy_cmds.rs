//! photocraft-clippy: the engine side of its Clip Studio Paint–style interface, the
//! `clippy` preferences and the `clippy.*` commands. PhotoCraft registers the commands too but
//! shows them nowhere (no menu item, no shortcut) and never reads the preferences: only the
//! `photocraft-clippy` build (crate `photocraft-clippy-ui`) draws what they change. See `CLIPPY.md`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// Switch the interface: `{"mode": "clippy"|"photoshop"}`.
pub const MODE_SET: &str = "clippy.mode.set";
/// Window › Interface › Clippy: the same switch without params, for the menu and shortcuts.
pub const MODE_CLIPPY: &str = "clippy.mode.clippy";
/// Window › Interface › Photoshop.
pub const MODE_PHOTOSHOP: &str = "clippy.mode.photoshop";
/// Store Clippy's dock layout: `{"layout": {...}|null}`.
pub const LAYOUT_SET: &str = "clippy.layout.set";
/// Window › Interface › Reset Clippy Workspace: back to the default layout.
pub const LAYOUT_RESET: &str = "clippy.layout.reset";
/// The key of Clippy's own layout in [`ClippyPrefs::layouts`] (PhotoCraft's interface keeps its
/// own `panelLayout`).
pub const LAYOUT_KEY: &str = "clippy";
/// Largest layout accepted, as JSON text: a full layout of every palette is a few KiB.
pub const MAX_LAYOUT_BYTES: usize = 256 * 1024;
/// Window › Workspace › New Workspace…: store the current layout under a name,
/// `{"name": "…", "overwrite"?: bool}`.
pub const WORKSPACE_SAVE: &str = "clippy.workspace.save";
/// Window › Workspace › a workspace: make its layout the current one, `{"name": "…"}`.
pub const WORKSPACE_LOAD: &str = "clippy.workspace.load";
/// Window › Workspace › Delete Workspace…: `{"name": "…"}`.
pub const WORKSPACE_DELETE: &str = "clippy.workspace.delete";
/// The saved workspaces' names: `{}` → `{"workspaces": [...]}`.
pub const WORKSPACE_LIST: &str = "clippy.workspace.list";
/// Longest workspace name, in characters.
pub const MAX_WORKSPACE_NAME: usize = 64;
/// Most workspaces kept (and read from the folder).
pub const MAX_WORKSPACES: usize = 256;
/// The `format` of a workspace file.
const WORKSPACE_FORMAT: &str = "photocraft-clippy-workspace";
/// Make a sub tool (`.sut`) the active one: `{"path": str}`.
pub const SUBTOOL_SELECT: &str = "clippy.subtool.select";
/// The active sub tool and how it maps to PhotoCraft.
pub const SUBTOOL_GET: &str = "clippy.subtool.get";
/// Write PhotoCraft's working copy (brush, tool options) into the active sub tool's `.sut`.
pub const SUBTOOL_WRITE_BACK: &str = "clippy.subtool.writeBack";
/// Back to the sub tool's initial settings (`NodeInitVariantID`).
pub const SUBTOOL_RESET: &str = "clippy.subtool.reset";
/// Make the current settings the sub tool's initial settings.
pub const SUBTOOL_SAVE_AS_DEFAULT: &str = "clippy.subtool.saveAsDefault";
/// Write PhotoCraft's current tool and brush as a new `.sut`: `{"path": str, "name": str?}`.
pub const SUBTOOL_EXPORT: &str = "clippy.subtool.export";
/// Every `clippy.subtool.*` command. `photocraft-clippy-ui` runs them (they read and write
/// files and drive the shell's tool); the engine only lists them.
pub const SUBTOOL_COMMANDS: [&str; 6] = [SUBTOOL_SELECT, SUBTOOL_GET, SUBTOOL_WRITE_BACK, SUBTOOL_RESET, SUBTOOL_SAVE_AS_DEFAULT, SUBTOOL_EXPORT];

/// Which interface Clippy shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClippyMode {
    /// Clippy's own interface (docks on every edge, sub tools).
    Clippy,
    /// PhotoCraft's Photoshop-like interface, unchanged.
    Photoshop,
}

/// Clippy's preferences (`Preferences::clippy`, JSON key `clippy`). Files without the key, such as
/// PhotoCraft's, load with these defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClippyPrefs {
    /// The interface chosen by the user; `None` (null) is the build's default, Clippy in
    /// `photocraft-clippy`. Null rather than a fixed default because Clippy's first start copies
    /// PhotoCraft's preferences, which never choose.
    pub mode: Option<ClippyMode>,
    /// Folder of the sub tool stock (an absolute path); empty for [`DEFAULT_STOCK_FOLDER`] in the
    /// settings folder. Read by the `clippy.stock.*` commands ([`stock_dir`]).
    pub stock_dir: String,
    /// The dock layout of each mode, by mode id (M2). JSON owned by `photocraft-clippy-ui`.
    pub layouts: Value,
    /// The active sub tool's `.sut` file (M8); empty for none. Set by `clippy.subtool.select`;
    /// the Clippy interface loads it at start and when it changes.
    pub subtool: String,
}

/// Clippy's session state outside the preferences: where the saved workspaces live.
#[derive(Clone, Debug, Default)]
pub struct ClippyState {
    /// The workspace folder (desktop: `<settings folder>/workspaces`, one `<name>.json` each).
    /// `None`, as in headless sessions, tests and the web, keeps workspaces in memory for the
    /// session.
    pub workspace_dir: Option<std::path::PathBuf>,
    /// The workspaces of a session without a folder: name → layout.
    memory: std::collections::BTreeMap<String, Value>,
}

impl ClippyMode {
    pub const ALL: [ClippyMode; 2] = [ClippyMode::Clippy, ClippyMode::Photoshop];

    /// The id used in params and the preferences file.
    pub fn id(self) -> &'static str {
        match self {
            ClippyMode::Clippy => "clippy",
            ClippyMode::Photoshop => "photoshop",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: MODE_SET,
            label: "Set Interface",
            menu: &[],
            shortcut: None,
            params: r##"{"mode":"clippy"|"photoshop"} → {mode}: the interface photocraft-clippy shows, switched at once and kept across restarts (preference clippy.mode; null there = clippy). PhotoCraft stores it but always shows its own interface"##,
            enabled: |_| Ok(()),
            run: mode_set,
            journal: false,
        },
        CommandSpec {
            id: MODE_CLIPPY,
            label: "Clippy",
            menu: &[],
            shortcut: None,
            params: r##"{} → {mode}: clippy.mode.set {"mode":"clippy"}"##,
            enabled: |_| Ok(()),
            run: |s, _| set_mode(s, ClippyMode::Clippy),
            journal: false,
        },
        CommandSpec {
            id: MODE_PHOTOSHOP,
            label: "Photoshop",
            menu: &[],
            shortcut: None,
            params: r##"{} → {mode}: clippy.mode.set {"mode":"photoshop"}"##,
            enabled: |_| Ok(()),
            run: |s, _| set_mode(s, ClippyMode::Photoshop),
            journal: false,
        },
        CommandSpec {
            id: LAYOUT_SET,
            label: "Set Clippy Layout",
            menu: &[],
            shortcut: None,
            params: r##"{"layout":{root,floating}|null} → {layout}: the dock layout of the Clippy interface (a craft_ui docking layout of palette ids such as "canvas", "layer", "subTool"; preference clippy.layouts.clippy). null is the default layout. Only its size and kind are checked here; photocraft-clippy shows the default for a layout it can't read"##,
            enabled: |_| Ok(()),
            run: layout_set,
            journal: false,
        },
        CommandSpec {
            id: LAYOUT_RESET,
            label: "Reset Clippy Workspace",
            menu: &[],
            shortcut: None,
            params: r##"{} → {layout: null}: the default Clippy dock layout"##,
            enabled: |_| Ok(()),
            run: |s, _| store_layout(s, Value::Null),
            journal: false,
        },
        CommandSpec {
            id: WORKSPACE_SAVE,
            label: "New Clippy Workspace",
            menu: &[],
            shortcut: None,
            params: r##"{"name":string,"overwrite"?:bool} → {name, layout}: store the current Clippy layout (preference clippy.layouts.clippy) as a workspace, a file <settings folder>/workspaces/<name>.json (in memory without a settings folder). Names are 1–64 characters without / \ : * ? " < > | or control characters and don't start with a dot; an existing workspace of that name (any case) is an error unless overwrite is true"##,
            enabled: |_| Ok(()),
            run: workspace_save,
            journal: false,
        },
        CommandSpec {
            id: WORKSPACE_LOAD,
            label: "Load Clippy Workspace",
            menu: &[],
            shortcut: None,
            params: r##"{"name":string} → {name, layout}: make a saved workspace's layout the current Clippy layout (clippy.layout.set); it stays after a restart"##,
            enabled: |_| Ok(()),
            run: workspace_load,
            journal: false,
        },
        CommandSpec {
            id: WORKSPACE_DELETE,
            label: "Delete Clippy Workspace",
            menu: &[],
            shortcut: None,
            params: r##"{"name":string} → {name}: delete a saved workspace (the current layout stays)"##,
            enabled: |_| Ok(()),
            run: workspace_delete,
            journal: false,
        },
        CommandSpec {
            id: WORKSPACE_LIST,
            label: "List Clippy Workspaces",
            menu: &[],
            shortcut: None,
            params: r##"{} → {workspaces: [name]}: the saved workspaces, sorted by name"##,
            enabled: |_| Ok(()),
            run: |s, _| Ok(json!({"workspaces": workspace_names(s)})),
            journal: false,
        },
        subtool_spec(
            SUBTOOL_SELECT,
            "Select Sub Tool",
            r##"{"path":str} → {path,name,tool,supported,notice?,…}: make the .sut at `path` the active sub tool. Its settings become PhotoCraft's tool, brush and tool options (the .sut is the source of truth; later changes are written back into it). A sub tool PhotoCraft can't map is selected with a notice and leaves the tool as it is. Preference clippy.subtool"##,
        ),
        subtool_spec(
            SUBTOOL_GET,
            "Sub Tool Info",
            r##"{} → {path,name,inputOp,outputOp,tool,supported,notice?,mapped:[rule],kept:[column]} or {path:null}: the active sub tool, the mapping rules that apply to it and the columns kept in the .sut without a PhotoCraft counterpart"##,
        ),
        subtool_spec(
            SUBTOOL_WRITE_BACK,
            "Write Back Sub Tool",
            r##"{} → {written:[rule]}: write PhotoCraft's brush and tool options into the active sub tool's .sut (only the columns of changed settings; the rest stays as it is). The Clippy interface does this by itself whenever a setting changed and the pointer is up"##,
        ),
        subtool_spec(
            SUBTOOL_RESET,
            "Reset Sub Tool",
            r##"{} → {path,…}: the active sub tool back to its initial settings (the .sut's NodeInitVariantID), saved and applied"##,
        ),
        subtool_spec(
            SUBTOOL_SAVE_AS_DEFAULT,
            "Register Sub Tool as Initial Settings",
            r##"{} → {path}: the active sub tool's current settings become its initial settings (copied into its NodeInitVariantID)"##,
        ),
        subtool_spec(
            SUBTOOL_EXPORT,
            "Export Sub Tool",
            r##"{"path":str,"name":str?} → {path,tool}: PhotoCraft's current tool, brush and tool options as a new .sut (for building sub tools from PhotoCraft settings)"##,
        ),
    ]
    .into_iter()
    .chain(stock_specs())
    .collect()
}

/// A workspace name from the params, trimmed and checked: it becomes a file name.
fn workspace_name(cmd: &str, p: &Value) -> Result<String> {
    let bad = |msg: String| EngineError::BadParams { cmd: cmd.into(), msg };
    let name = match p.get("name") {
        Some(Value::String(name)) => name.trim(),
        Some(_) => return Err(bad("`name` must be a string".into())),
        None => return Err(bad("missing `name`".into())),
    };
    let count = name.chars().count();
    if count == 0 || count > MAX_WORKSPACE_NAME {
        return Err(bad(format!("`name` must have 1–{MAX_WORKSPACE_NAME} characters")));
    }
    if name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)) || name.starts_with('.') || name.ends_with('.') {
        return Err(bad(format!("`{name}` can't be a workspace name: no / \\ : * ? \" < > |, control characters or dot at either end")));
    }
    Ok(name.to_string())
}

/// The saved workspace whose name matches `name` regardless of case (file systems may not tell
/// them apart).
fn find_workspace(s: &Session, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    workspace_names(s).into_iter().find(|n| n.to_lowercase() == lower)
}

/// The saved workspaces' names, sorted case-insensitively; at most [`MAX_WORKSPACES`].
pub fn workspace_names(s: &Session) -> Vec<String> {
    let mut names: Vec<String> = match s.clippy.workspace_dir.as_deref() {
        Some(dir) => store::names(dir),
        None => s.clippy.memory.keys().cloned().collect(),
    };
    names.retain(|n| workspace_name("", &json!({"name": n})).is_ok_and(|ok| &ok == n));
    names.sort_by_key(|n| (n.to_lowercase(), n.clone()));
    names.truncate(MAX_WORKSPACES);
    names
}

fn workspace_save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = workspace_name(WORKSPACE_SAVE, p)?;
    let overwrite = match p.get("overwrite") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(EngineError::BadParams { cmd: WORKSPACE_SAVE.into(), msg: "`overwrite` must be true or false".into() }),
    };
    let existing = find_workspace(s, &name);
    if existing.is_some() && !overwrite {
        return Err(EngineError::Other(format!("a workspace named `{name}` exists; pass \"overwrite\": true to replace it")));
    }
    if existing.is_none() && workspace_names(s).len() >= MAX_WORKSPACES {
        return Err(EngineError::Other(format!("there are {MAX_WORKSPACES} workspaces already; delete one first")));
    }
    // An existing one keeps its file (and so its spelling).
    let name = existing.unwrap_or(name);
    let layout = s.prefs().clippy.layouts.get(LAYOUT_KEY).cloned().unwrap_or(Value::Null);
    match s.clippy.workspace_dir.clone() {
        Some(dir) => {
            let file = json!({"format": WORKSPACE_FORMAT, "version": 1, "name": name, "layout": layout});
            store::write(&dir, &name, &file).map_err(EngineError::Other)?;
        }
        None => {
            s.clippy.memory.insert(name.clone(), layout.clone());
        }
    }
    Ok(json!({"name": name, "layout": layout}))
}

fn workspace_load(s: &mut Session, p: &Value) -> Result<Value> {
    let asked = workspace_name(WORKSPACE_LOAD, p)?;
    let name = find_workspace(s, &asked).ok_or_else(|| EngineError::Other(format!("no workspace named `{asked}`")))?;
    let layout = match s.clippy.workspace_dir.as_deref() {
        Some(dir) => {
            let file = store::read(dir, &name).map_err(EngineError::Other)?;
            match file.get("layout") {
                Some(v @ (Value::Object(_) | Value::Null)) => v.clone(),
                _ => return Err(EngineError::Other(format!("the workspace `{name}` has no layout"))),
            }
        }
        None => s.clippy.memory.get(&name).cloned().unwrap_or(Value::Null),
    };
    let out = layout_set(s, &json!({"layout": layout}))?;
    Ok(json!({"name": name, "layout": out.get("layout").cloned().unwrap_or(Value::Null)}))
}

fn workspace_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let asked = workspace_name(WORKSPACE_DELETE, p)?;
    let name = find_workspace(s, &asked).ok_or_else(|| EngineError::Other(format!("no workspace named `{asked}`")))?;
    match s.clippy.workspace_dir.as_deref() {
        Some(dir) => store::remove(dir, &name).map_err(EngineError::Other)?,
        None => {
            s.clippy.memory.remove(&name);
        }
    }
    Ok(json!({"name": name}))
}

/// The workspace folder on disk. Every read is bounded; an unreadable file is skipped or an
/// error, never a panic.
#[cfg(not(target_arch = "wasm32"))]
mod store {
    use std::path::{Path, PathBuf};

    use serde_json::Value;

    /// Largest workspace file read: a layout plus its name.
    const MAX_FILE_BYTES: u64 = super::MAX_LAYOUT_BYTES as u64 + 4096;

    fn path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.json"))
    }

    /// The names of the `*.json` files in `dir` (none when it doesn't exist yet).
    pub fn names(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries
            .filter_map(|e| e.ok())
            .take(super::MAX_WORKSPACES.saturating_mul(4))
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_string))
            .collect()
    }

    pub fn read(dir: &Path, name: &str) -> Result<Value, String> {
        use std::io::Read;
        let path = path(dir, name);
        let file = std::fs::File::open(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut text = String::new();
        file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(format!("{} is larger than a workspace can be", path.display()));
        }
        let value: Value = serde_json::from_str(&text).map_err(|e| format!("{} is not a workspace: {e}", path.display()))?;
        if value.get("format").and_then(Value::as_str) != Some(super::WORKSPACE_FORMAT) {
            return Err(format!("{} is not a photocraft-clippy workspace", path.display()));
        }
        Ok(value)
    }

    pub fn write(dir: &Path, name: &str, value: &Value) -> Result<(), String> {
        let path = path(dir, name);
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        photocraft_format::atomic_write(&path, text.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn remove(dir: &Path, name: &str) -> Result<(), String> {
        let path = path(dir, name);
        std::fs::remove_file(&path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
    }
}

/// The web has no folder: [`ClippyState::workspace_dir`] is never set there.
#[cfg(target_arch = "wasm32")]
mod store {
    use std::path::Path;

    use serde_json::Value;

    pub fn names(_: &Path) -> Vec<String> {
        Vec::new()
    }

    pub fn read(_: &Path, name: &str) -> Result<Value, String> {
        Err(format!("no workspace named `{name}`"))
    }

    pub fn write(_: &Path, _: &str, _: &Value) -> Result<(), String> {
        Err("this build can't write files".into())
    }

    pub fn remove(_: &Path, name: &str) -> Result<(), String> {
        Err(format!("no workspace named `{name}`"))
    }
}

/// A `clippy.subtool.*` command. Without photocraft-clippy's interface (PhotoCraft, the CLI) it
/// reports that it isn't available; the interface runs it before the engine is asked.
fn subtool_spec(id: &'static str, label: &'static str, params: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params,
        enabled: |_| Ok(()),
        run: |_, _| Err(EngineError::Other("sub tools (.sut) are only available in photocraft-clippy's Clippy interface".into())),
        journal: false,
    }
}

fn layout_set(s: &mut Session, p: &Value) -> Result<Value> {
    let bad = |msg: String| EngineError::BadParams { cmd: LAYOUT_SET.into(), msg };
    let layout = match p.get("layout") {
        Some(v @ (Value::Object(_) | Value::Null)) => v.clone(),
        Some(_) => return Err(bad("`layout` must be an object or null".into())),
        None => return Err(bad("missing `layout` (an object, or null for the default)".into())),
    };
    let size = serde_json::to_string(&layout).map(|t| t.len()).unwrap_or(usize::MAX);
    if size > MAX_LAYOUT_BYTES {
        return Err(bad(format!("`layout` is {size} bytes of JSON; at most {MAX_LAYOUT_BYTES}")));
    }
    store_layout(s, layout)
}

/// Store the layout under [`LAYOUT_KEY`], keeping other keys; the preferences revision moves
/// only on a change.
fn store_layout(s: &mut Session, layout: Value) -> Result<Value> {
    if s.prefs().clippy.layouts.get(LAYOUT_KEY).unwrap_or(&Value::Null) != &layout {
        s.edit_prefs(|p| {
            if !p.clippy.layouts.is_object() {
                p.clippy.layouts = json!({});
            }
            if let Some(map) = p.clippy.layouts.as_object_mut() {
                map.insert(LAYOUT_KEY.into(), layout.clone());
            }
        });
    }
    Ok(json!({"layout": layout}))
}

fn mode_set(s: &mut Session, p: &Value) -> Result<Value> {
    let bad = |msg: String| EngineError::BadParams { cmd: MODE_SET.into(), msg };
    let ids = || ClippyMode::ALL.map(ClippyMode::id).join("|");
    let mode = match p.get("mode") {
        Some(Value::String(id)) => ClippyMode::parse(id).ok_or_else(|| bad(format!("`mode` must be one of {} (got `{id}`)", ids())))?,
        Some(_) => return Err(bad(format!("`mode` must be a string: {}", ids()))),
        None => return Err(bad(format!("missing `mode` ({})", ids()))),
    };
    set_mode(s, mode)
}

/// Store the mode; the interface reads it every frame, so it switches without a restart. The
/// preferences revision moves only on a change, so choosing the current mode saves nothing.
fn set_mode(s: &mut Session, mode: ClippyMode) -> Result<Value> {
    if s.prefs().clippy.mode != Some(mode) {
        s.edit_prefs(|p| p.clippy.mode = Some(mode));
    }
    Ok(json!({"mode": mode.id()}))
}

// ---------- the sub tool stock (M7) ----------
//
// The stock is a folder of tool and group folders holding `.sut` files (crate
// `photocraft-clippy-stock`, CLIPPY.md "Sub tool stock"). These commands change it on disk; they
// keep no state, so a file manager and Clippy can work on the same folder. Stock paths name
// entries by folder and file names: `Tool`, `Tool/Group`, `Tool/Group/Sub tool`.

/// The loaded stock: `{}` → `{root, default, tools, warnings}`.
pub const STOCK_LIST: &str = "clippy.stock.list";
/// New tool folder: `{"name"}`.
pub const STOCK_TOOL_CREATE: &str = "clippy.stock.tool.create";
/// New group folder: `{"tool", "name"}`.
pub const STOCK_GROUP_CREATE: &str = "clippy.stock.group.create";
/// Rename a tool, group or sub tool: `{"path", "name"}`.
pub const STOCK_RENAME: &str = "clippy.stock.rename";
/// Move an entry to the trash: `{"path"}`.
pub const STOCK_DELETE: &str = "clippy.stock.delete";
/// Move an entry: `{"path", "to"?, "index"?}`.
pub const STOCK_MOVE: &str = "clippy.stock.move";
/// Copy an entry (an independent copy): `{"path", "to"?, "index"?}`.
pub const STOCK_COPY: &str = "clippy.stock.copy";
/// Order the entries of a folder: `{"path"?, "order"?}`.
pub const STOCK_SORT: &str = "clippy.stock.sort";
/// Display name, icon, shortcut: `{"path", "name"?, "icon"?, "shortcut"?}`.
pub const STOCK_META_SET: &str = "clippy.stock.meta.set";
/// Import a `.sut`, `.todb` or `.abr`: `{"file", …}`.
pub const STOCK_IMPORT: &str = "clippy.stock.import";
/// Export the stock or one entry as a copy: `{"path"?, "to"}`.
pub const STOCK_EXPORT: &str = "clippy.stock.export";
/// Save Brushes As: copy the stock to `{"to"}` and work there.
pub const STOCK_SAVE_AS: &str = "clippy.stock.saveAs";
/// The folder inside the settings folder that holds the stock when `clippy.stockDir` is empty.
pub const DEFAULT_STOCK_FOLDER: &str = "stock";
/// Longest file-system path accepted in params, in bytes.
pub const MAX_PATH_BYTES: usize = 4096;

/// The app's settings folder, for the default stock (`<settings>/stock`). Set once by the app at
/// start ([`set_settings_dir`]); headless sessions without it need `clippy.stockDir`.
static SETTINGS_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Tells the stock commands where the app keeps its settings: the default stock is the folder
/// [`DEFAULT_STOCK_FOLDER`] in it.
pub fn set_settings_dir(dir: Option<std::path::PathBuf>) {
    *SETTINGS_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = dir;
}

/// The stock folder and whether it is the default one: `clippy.stockDir` (an absolute path) when
/// set, else [`DEFAULT_STOCK_FOLDER`] in the settings folder.
pub fn stock_dir(s: &Session) -> Result<(std::path::PathBuf, bool)> {
    let chosen = s.prefs().clippy.stock_dir.trim();
    if !chosen.is_empty() {
        let path = std::path::PathBuf::from(chosen);
        if !path.is_absolute() {
            return Err(EngineError::Other(format!("clippy.stockDir `{chosen}` is not an absolute path; choose the stock folder again")));
        }
        return Ok((path, false));
    }
    let settings = SETTINGS_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    settings
        .map(|d| (d.join(DEFAULT_STOCK_FOLDER), true))
        .ok_or_else(|| EngineError::Other("no stock folder: set clippy.stockDir to a folder for Clippy's sub tools".into()))
}

fn stock_specs() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params,
        enabled: |_| Ok(()),
        run,
        journal: false,
    };
    vec![
        spec(
            STOCK_LIST,
            "Sub Tool Stock",
            r##"{} → {root, default, tools:[{dir, name, icon, shortcut, warning, groups:[{dir, name, icon, shortcut, warning, subtools:[{file, name, nodeName, uuid, shortcut, warning}]}]}], warnings}: the sub tool stock as on disk (clippy.stockDir, else "stock" in the settings folder), in display order. A broken .sut is an entry with a warning; unknown files are ignored"##,
            stock::list,
        ),
        spec(STOCK_TOOL_CREATE, "New Tool", r##"{"name"} → {path}: a new tool folder, last; a clash gets a number ("Pen 2")"##, stock::tool_create),
        spec(
            STOCK_GROUP_CREATE,
            "New Sub Tool Group",
            r##"{"tool","name"} → {path}: a new group folder in a tool, last; a clash gets a number"##,
            stock::group_create,
        ),
        spec(
            STOCK_RENAME,
            "Rename",
            r##"{"path":"Tool[/Group[/Sub tool]]","name"} → {path}: renames the folder or .sut (and the name inside it); another entry of that name is an error"##,
            stock::rename,
        ),
        spec(STOCK_DELETE, "Delete", r##"{"path"} → {deleted}: moves a tool, group or sub tool to the trash"##, stock::delete),
        spec(
            STOCK_MOVE,
            "Move",
            r##"{"path","to"?:"Tool[/Group]"|"","index"?} → {path}: a sub tool into a group, a group into a tool, a tool within the stock ("to" empty or left out); within its own folder it is a reorder. A clash gets a number"##,
            stock::move_entry,
        ),
        spec(
            STOCK_COPY,
            "Duplicate",
            r##"{"path","to"?,"index"?} → {path}: an independent copy (fresh node UUIDs) into a folder, which may be its own ("G-Pen 2")"##,
            stock::copy_entry,
        ),
        spec(
            STOCK_SORT,
            "Sort",
            r##"{"path"?:"Tool[/Group]"|"","order"?:[names]|null} → {order}: the order of a folder's entries (the stock's tools without "path"); entries left out follow alphabetically, null sorts all alphabetically"##,
            stock::sort,
        ),
        spec(
            STOCK_META_SET,
            "Set Sub Tool Metadata",
            r##"{"path","name"?,"icon"?,"shortcut"?} → {path,name,icon,shortcut}: display name, icon and shortcut of a tool or group, the shortcut of a sub tool; null clears"##,
            stock::meta_set,
        ),
        spec(
            STOCK_IMPORT,
            "Import Sub Tools",
            r##"{"file", "group"?:"Tool/Group", "materials"?, "tool"?, "name"?} → {kind, …report}: a .sut into "group"; a Clip Studio Paint .todb as tools and groups, with brush tips from its material folder "materials"; an .abr as a new group "name" (the file name) in the brush tool ("tool" to choose one)"##,
            stock::import,
        ),
        spec(
            STOCK_EXPORT,
            "Export Sub Tools",
            r##"{"path"?,"to"} → {path}: a copy of the whole stock, or of one tool, group or sub tool, in the folder "to""##,
            stock::export,
        ),
        spec(
            STOCK_SAVE_AS,
            "Save Brushes As",
            r##"{"to"} → {stockDir}: copies the whole stock to the new or empty folder "to" and works there from now on (clippy.stockDir); the old folder stays as it was"##,
            stock::save_as,
        ),
    ]
}

#[cfg(target_arch = "wasm32")]
mod stock {
    use serde_json::Value;

    use crate::{EngineError, Result, Session};

    macro_rules! web_only {
        ($($name:ident),*) => {$(
            pub(super) fn $name(_: &mut Session, _: &Value) -> Result<Value> {
                Err(EngineError::Other("the sub tool stock needs the desktop app".into()))
            }
        )*};
    }
    web_only!(list, tool_create, group_create, rename, delete, move_entry, copy_entry, sort, meta_set, import, export, save_as);
}

#[cfg(not(target_arch = "wasm32"))]
mod stock {
    use std::path::{Path, PathBuf};

    use photocraft_clippy_stock::ops::{self, MetaChange};
    use photocraft_clippy_stock::trash::Trash;
    use photocraft_clippy_stock::{EntryPath, Error as StockError, Stock, export as stock_export, import as stock_import};
    use serde_json::{Value, json};

    use super::{
        MAX_PATH_BYTES, STOCK_COPY, STOCK_DELETE, STOCK_EXPORT, STOCK_GROUP_CREATE, STOCK_IMPORT, STOCK_META_SET, STOCK_MOVE, STOCK_RENAME, STOCK_SAVE_AS,
    };
    use super::{STOCK_SORT, STOCK_TOOL_CREATE, stock_dir};
    use crate::{EngineError, Result, Session};

    #[cfg(test)]
    thread_local! {
        /// Tests delete into their own trash folder, not the user's.
        pub(super) static TEST_TRASH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    }

    fn trash() -> Trash {
        #[cfg(test)]
        if let Some(dir) = TEST_TRASH.with(|t| t.borrow().clone()) {
            return Trash::Freedesktop(dir);
        }
        Trash::System
    }

    fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
        EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
    }

    /// A stock error as the engine reports it: bad names and paths are bad params.
    fn err(cmd: &'static str) -> impl Fn(StockError) -> EngineError {
        move |e| match e {
            StockError::BadName { .. } | StockError::BadPath { .. } => bad(cmd, e.to_string()),
            other => EngineError::Other(other.to_string()),
        }
    }

    fn string<'a>(p: &'a Value, key: &str, cmd: &str) -> Result<&'a str> {
        match p.get(key) {
            Some(Value::String(s)) => Ok(s),
            Some(_) => Err(bad(cmd, format!("`{key}` must be a string"))),
            None => Err(bad(cmd, format!("missing `{key}`"))),
        }
    }

    fn entry(p: &Value, key: &str, cmd: &'static str) -> Result<EntryPath> {
        EntryPath::parse(string(p, key, cmd)?).map_err(err(cmd))
    }

    /// An optional folder path: missing, null or "" is the stock itself.
    fn folder(p: &Value, key: &str, cmd: &'static str) -> Result<Option<EntryPath>> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) if s.is_empty() => Ok(None),
            Some(Value::String(s)) => EntryPath::parse(s).map(Some).map_err(err(cmd)),
            Some(_) => Err(bad(cmd, format!("`{key}` must be a stock path (a string)"))),
        }
    }

    fn index(p: &Value, cmd: &str) -> Result<Option<usize>> {
        match p.get("index") {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_u64().and_then(|i| usize::try_from(i).ok()).map(Some).ok_or_else(|| bad(cmd, "`index` must be a whole number from 0")),
        }
    }

    /// A file-system path param (a file or folder the user chose).
    fn fs_path(p: &Value, key: &str, cmd: &str) -> Result<PathBuf> {
        let s = string(p, key, cmd)?;
        if s.trim().is_empty() || s.contains('\0') || s.len() > MAX_PATH_BYTES {
            return Err(bad(cmd, format!("`{key}` must be a path: not empty, no NUL, at most {MAX_PATH_BYTES} bytes")));
        }
        Ok(PathBuf::from(s))
    }

    /// The stock folder for an operation; the default one is created on first use.
    fn root(s: &Session) -> Result<PathBuf> {
        let (root, default) = stock_dir(s)?;
        if default {
            ops::ensure_root(&root).map_err(|e| EngineError::Other(e.to_string()))?;
        } else if !root.is_dir() {
            return Err(EngineError::Other(format!("the stock folder {} does not exist; choose another (clippy.stockDir)", root.display())));
        }
        Ok(root)
    }

    fn path_json(path: &EntryPath) -> Value {
        json!({"path": path.to_string(), "kind": path.kind()})
    }

    pub(super) fn list(s: &mut Session, _: &Value) -> Result<Value> {
        let (root, default) = stock_dir(s)?;
        if default && !root.exists() {
            return Ok(json!({"root": root, "default": true, "tools": [], "warnings": []}));
        }
        let stock = Stock::load(&root).map_err(|e| EngineError::Other(format!("{e}; choose another stock folder (clippy.stockDir)")))?;
        let mut v = serde_json::to_value(&stock).map_err(|e| EngineError::Other(e.to_string()))?;
        if let Some(o) = v.as_object_mut() {
            o.insert("default".into(), json!(default));
        }
        Ok(v)
    }

    pub(super) fn tool_create(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_TOOL_CREATE;
        let name = string(p, "name", cmd)?;
        photocraft_clippy_stock::names::validate(name).map_err(err(cmd))?;
        let root = root(s)?;
        Ok(path_json(&ops::create_tool(&root, name).map_err(err(cmd))?))
    }

    pub(super) fn group_create(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_GROUP_CREATE;
        let (tool, name) = (string(p, "tool", cmd)?, string(p, "name", cmd)?);
        for n in [tool, name] {
            photocraft_clippy_stock::names::validate(n).map_err(err(cmd))?;
        }
        let root = root(s)?;
        Ok(path_json(&ops::create_group(&root, tool, name).map_err(err(cmd))?))
    }

    pub(super) fn rename(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_RENAME;
        let (path, name) = (entry(p, "path", cmd)?, string(p, "name", cmd)?);
        photocraft_clippy_stock::names::validate(name).map_err(err(cmd))?;
        let root = root(s)?;
        Ok(path_json(&ops::rename(&root, &path, name).map_err(err(cmd))?))
    }

    pub(super) fn delete(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_DELETE;
        let path = entry(p, "path", cmd)?;
        let root = root(s)?;
        ops::delete(&root, &path, &trash()).map_err(err(cmd))?;
        Ok(json!({"deleted": path.to_string()}))
    }

    pub(super) fn move_entry(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_MOVE;
        let (path, to, index) = (entry(p, "path", cmd)?, folder(p, "to", cmd)?, index(p, cmd)?);
        let root = root(s)?;
        Ok(path_json(&ops::move_entry(&root, &path, to.as_ref(), index).map_err(err(cmd))?))
    }

    pub(super) fn copy_entry(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_COPY;
        let (path, to, index) = (entry(p, "path", cmd)?, folder(p, "to", cmd)?, index(p, cmd)?);
        let root = root(s)?;
        Ok(path_json(&ops::copy_entry(&root, &path, to.as_ref(), index).map_err(err(cmd))?))
    }

    pub(super) fn sort(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_SORT;
        let parent = folder(p, "path", cmd)?;
        let order = match p.get("order") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) if items.len() <= photocraft_clippy_stock::MAX_FOLDER_ENTRIES => Some(
                items
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).ok_or_else(|| bad(cmd, "`order` must list entry names (strings)")))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Some(_) => Err(bad(cmd, "`order` must be a list of entry names, or null"))?,
        };
        let root = root(s)?;
        let order = ops::sort(&root, parent.as_ref(), order.as_deref()).map_err(err(cmd))?;
        Ok(json!({"order": order}))
    }

    /// `Some(Some(text))`, `Some(None)` for null, `None` when the key is missing.
    fn change(p: &Value, key: &str, cmd: &str) -> Result<Option<Option<String>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(t)) => Ok(Some(Some(t.chars().take(photocraft_clippy_stock::meta::MAX_TEXT_CHARS).collect()))),
            Some(_) => Err(bad(cmd, format!("`{key}` must be a string or null"))),
        }
    }

    pub(super) fn meta_set(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_META_SET;
        let path = entry(p, "path", cmd)?;
        let change = MetaChange { name: change(p, "name", cmd)?, icon: change(p, "icon", cmd)?, shortcut: change(p, "shortcut", cmd)? };
        if change == MetaChange::default() {
            return Err(bad(cmd, "give `name`, `icon` or `shortcut`"));
        }
        let root = root(s)?;
        let meta = ops::set_meta(&root, &path, &change).map_err(err(cmd))?;
        let shortcut = match &path {
            EntryPath::SubTool(..) => meta.shortcuts.get(&path.file_name()).cloned(),
            _ => meta.shortcut.clone(),
        };
        Ok(json!({"path": path.to_string(), "name": meta.name, "icon": meta.icon, "shortcut": shortcut}))
    }

    pub(super) fn import(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_IMPORT;
        let file = fs_path(p, "file", cmd)?;
        let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        match ext.as_str() {
            "sut" => {
                let group = match entry(p, "group", cmd)? {
                    EntryPath::Group(t, g) => (t, g),
                    other => return Err(bad(cmd, format!("`group` must be Tool/Group, not the {} {other}", other.kind()))),
                };
                let root = root(s)?;
                let path = stock_import::import_sut(&root, &file, &group.0, &group.1).map_err(err(cmd))?;
                Ok(json!({"kind": "sut", "subtools": [path.to_string()]}))
            }
            "todb" => {
                let materials = match p.get("materials") {
                    None | Some(Value::Null) => None,
                    Some(_) => Some(fs_path(p, "materials", cmd)?),
                };
                let root = root(s)?;
                let report = stock_import::import_todb(&root, &file, materials.as_deref()).map_err(err(cmd))?;
                report_json("todb", &report)
            }
            "abr" => import_abr(s, p, &file),
            _ => Err(bad(cmd, "`file` must be a .sut, .todb or .abr file")),
        }
    }

    fn import_abr(s: &mut Session, p: &Value, file: &Path) -> Result<Value> {
        let cmd = STOCK_IMPORT;
        let tool = match p.get("tool") {
            None | Some(Value::Null) => None,
            Some(_) => Some(string(p, "tool", cmd)?),
        };
        if let Some(t) = tool {
            photocraft_clippy_stock::names::validate(t).map_err(err(cmd))?;
        }
        let stem = file.file_stem().and_then(|n| n.to_str()).unwrap_or("Brushes").to_owned();
        let name = match p.get("name") {
            None | Some(Value::Null) => stem,
            Some(_) => string(p, "name", cmd)?.to_owned(),
        };
        let len = std::fs::metadata(file).map_err(|e| EngineError::Other(format!("{}: {e}", file.display())))?.len();
        if len > crate::preset_import_cmds::MAX_FILE_BYTES {
            return Err(bad(cmd, format!("{}: file too large ({len} bytes)", file.display())));
        }
        let bytes = std::fs::read(file).map_err(|e| EngineError::Other(format!("{}: {e}", file.display())))?;
        let imported = photocraft_io::abr_map::read_abr(&bytes, &name).map_err(|e| bad(cmd, format!("{}: {e}", file.display())))?;
        let root = root(s)?;
        let (group, mut report) = stock_import::import_presets(&root, tool, &name, &imported.presets).map_err(err(cmd))?;
        report.warnings.extend(imported.warnings);
        let mut v = report_json("abr", &report)?;
        if let Some(o) = v.as_object_mut() {
            o.insert("group".into(), json!(group.to_string()));
        }
        Ok(v)
    }

    fn report_json(kind: &str, report: &stock_import::ImportReport) -> Result<Value> {
        let mut v = serde_json::to_value(report).map_err(|e| EngineError::Other(e.to_string()))?;
        if let Some(o) = v.as_object_mut() {
            o.insert("kind".into(), json!(kind));
        }
        Ok(v)
    }

    pub(super) fn export(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_EXPORT;
        let path = folder(p, "path", cmd)?;
        let to = fs_path(p, "to", cmd)?;
        let (root, _) = stock_dir(s)?;
        let out = stock_export::export(&root, path.as_ref(), &to).map_err(err(cmd))?;
        Ok(json!({"path": out}))
    }

    pub(super) fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
        let cmd = STOCK_SAVE_AS;
        let to = fs_path(p, "to", cmd)?;
        let to = std::path::absolute(&to).map_err(|e| bad(cmd, format!("`to`: {e}")))?;
        let to_text = to.to_str().ok_or_else(|| bad(cmd, "`to` must be a UTF-8 path"))?.to_owned();
        let (root, _) = stock_dir(s)?;
        stock_export::save_as(&root, &to).map_err(err(cmd))?;
        if s.prefs().clippy.stock_dir != to_text {
            s.edit_prefs(|p| p.clippy.stock_dir = to_text.clone());
        }
        Ok(json!({"stockDir": to_text}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip_through_their_ids() {
        for m in ClippyMode::ALL {
            assert_eq!(ClippyMode::parse(m.id()), Some(m));
            assert_eq!(serde_json::to_value(m).unwrap(), json!(m.id()));
        }
        assert_eq!(ClippyMode::parse("Clippy"), None);
    }

    #[test]
    fn set_switches_and_reports_the_mode() {
        let mut s = Session::new();
        assert_eq!(s.prefs().clippy.mode, None);
        assert_eq!(s.execute(MODE_SET, json!({"mode": "photoshop"})).unwrap(), json!({"mode": "photoshop"}));
        assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Photoshop));
        assert_eq!(s.execute(MODE_CLIPPY, json!({})).unwrap(), json!({"mode": "clippy"}));
        assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Clippy));
        assert_eq!(s.execute(MODE_PHOTOSHOP, Value::Null).unwrap(), json!({"mode": "photoshop"}));
        assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Photoshop));
    }

    #[test]
    fn set_moves_the_preferences_revision_only_on_a_change() {
        let mut s = Session::new();
        s.execute(MODE_SET, json!({"mode": "clippy"})).unwrap();
        let rev = s.prefs.rev();
        s.execute(MODE_CLIPPY, json!({})).unwrap();
        assert_eq!(s.prefs.rev(), rev);
        s.execute(MODE_PHOTOSHOP, json!({})).unwrap();
        assert!(s.prefs.rev() > rev);
    }

    #[test]
    fn bad_params_are_errors_and_change_nothing() {
        let mut s = Session::new();
        s.execute(MODE_SET, json!({"mode": "photoshop"})).unwrap();
        for p in [
            json!({}),
            Value::Null,
            json!([]),
            json!("clippy"),
            json!({"mode": null}),
            json!({"mode": 1}),
            json!({"mode": ""}),
            json!({"mode": "Clippy"}),
            json!({"mode": "csp"}),
            json!({"mode": ["clippy"]}),
            json!({"mode": "clippy\u{0}"}),
        ] {
            let err = s.execute(MODE_SET, p.clone()).unwrap_err();
            assert!(matches!(err, EngineError::BadParams { .. }), "{p}: {err}");
            assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Photoshop), "{p}");
        }
    }

    #[test]
    fn prefs_round_trip_and_older_files_load() {
        let mut s = Session::new();
        s.execute(MODE_SET, json!({"mode": "photoshop"})).unwrap();
        let saved = s.prefs_to_json();
        let mut t = Session::new();
        t.load_prefs_json(&saved).unwrap();
        assert_eq!(t.prefs().clippy, s.prefs().clippy);
        // PhotoCraft's own files have no `clippy` key: the build's default mode.
        let mut u = Session::new();
        u.load_prefs_json(r#"{"workspaceLocked": true}"#).unwrap();
        assert!(u.prefs().workspace_locked);
        assert_eq!(u.prefs().clippy, ClippyPrefs::default());
        assert_eq!(u.prefs().clippy.mode, None);
    }

    #[test]
    fn prefs_set_validates_the_mode() {
        let mut s = Session::new();
        s.execute("prefs.set", json!({"path": "clippy.mode", "value": "photoshop"})).unwrap();
        assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Photoshop));
        assert!(s.execute("prefs.set", json!({"path": "clippy.mode", "value": "csp"})).is_err());
        assert_eq!(s.prefs().clippy.mode, Some(ClippyMode::Photoshop));
        // null goes back to the build's default.
        s.execute("prefs.set", json!({"path": "clippy.mode", "value": null})).unwrap();
        assert_eq!(s.prefs().clippy.mode, None);
        assert_eq!(s.execute("prefs.get", json!({"path": "clippy.mode"})).unwrap(), Value::Null);
    }

    #[test]
    fn the_layout_is_stored_and_reset() {
        let mut s = Session::new();
        let layout = json!({"root": {"Tabs": {"panels": ["canvas"], "active": 0}}, "floating": []});
        assert_eq!(s.execute(LAYOUT_SET, json!({"layout": layout})).unwrap(), json!({"layout": layout}));
        assert_eq!(s.prefs().clippy.layouts, json!({"clippy": layout}));
        let rev = s.prefs.rev();
        s.execute(LAYOUT_SET, json!({"layout": layout})).unwrap();
        assert_eq!(s.prefs.rev(), rev, "the same layout saves nothing");
        // Other modes' entries stay.
        s.edit_prefs(|p| p.clippy.layouts["other"] = json!(1));
        assert_eq!(s.execute(LAYOUT_RESET, json!({})).unwrap(), json!({"layout": null}));
        assert_eq!(s.prefs().clippy.layouts, json!({"clippy": null, "other": 1}));
        // A file whose `layouts` isn't an object gets one.
        s.edit_prefs(|p| p.clippy.layouts = json!([1]));
        s.execute(LAYOUT_SET, json!({"layout": layout})).unwrap();
        assert_eq!(s.prefs().clippy.layouts, json!({"clippy": layout}));
    }

    #[test]
    fn bad_layouts_are_errors_and_change_nothing() {
        let mut s = Session::new();
        let huge = json!({"x": "a".repeat(MAX_LAYOUT_BYTES)});
        for p in [json!({}), Value::Null, json!([]), json!({"layout": 1}), json!({"layout": "x"}), json!({"layout": []}), json!({"layout": huge})] {
            let err = s.execute(LAYOUT_SET, p.clone()).unwrap_err();
            assert!(matches!(err, EngineError::BadParams { .. }), "{p}: {err}");
            assert_eq!(s.prefs().clippy.layouts, Value::Null, "{p}");
        }
    }

    fn layout(panel: &str) -> Value {
        json!({"root": {"Split": {"axis": "Horizontal", "size": {"FixedFirst": 200.0}, "first": {"Tabs": {"panels": [panel], "active": 0}}, "second": {"Tabs": {"panels": ["canvas"], "active": 0}}}}, "floating": []})
    }

    /// Save, list, load and delete, in memory and in a folder.
    #[test]
    fn workspaces_save_load_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        for folder in [None, Some(dir.path().join("workspaces"))] {
            let mut s = Session::new();
            s.clippy.workspace_dir = folder.clone();
            assert_eq!(s.execute(WORKSPACE_LIST, json!({})).unwrap(), json!({"workspaces": []}));
            s.execute(LAYOUT_SET, json!({"layout": layout("layer")})).unwrap();
            assert_eq!(s.execute(WORKSPACE_SAVE, json!({"name": " Painting "})).unwrap(), json!({"name": "Painting", "layout": layout("layer")}));
            s.execute(LAYOUT_SET, json!({"layout": layout("info")})).unwrap();
            s.execute(WORKSPACE_SAVE, json!({"name": "comic"})).unwrap();
            assert_eq!(s.execute(WORKSPACE_LIST, Value::Null).unwrap(), json!({"workspaces": ["comic", "Painting"]}));
            if let Some(folder) = &folder {
                let text = std::fs::read_to_string(folder.join("Painting.json")).unwrap();
                assert!(text.contains("photocraft-clippy-workspace") && text.contains("\"layer\""), "{text}");
            }

            // Loading sets the current layout, any case of the name finds it.
            assert_eq!(s.execute(WORKSPACE_LOAD, json!({"name": "painting"})).unwrap()["name"], "Painting");
            assert_eq!(s.prefs().clippy.layouts[LAYOUT_KEY], layout("layer"));
            // The same name again needs `overwrite`, and keeps its spelling.
            assert!(matches!(s.execute(WORKSPACE_SAVE, json!({"name": "COMIC"})), Err(EngineError::Other(_))));
            assert_eq!(s.execute(WORKSPACE_SAVE, json!({"name": "COMIC", "overwrite": true})).unwrap()["name"], "comic");
            s.execute(WORKSPACE_LOAD, json!({"name": "comic"})).unwrap();
            assert_eq!(s.prefs().clippy.layouts[LAYOUT_KEY], layout("layer"));

            assert_eq!(s.execute(WORKSPACE_DELETE, json!({"name": "Painting"})).unwrap(), json!({"name": "Painting"}));
            assert_eq!(s.execute(WORKSPACE_LIST, json!({})).unwrap(), json!({"workspaces": ["comic"]}));
            assert!(s.execute(WORKSPACE_LOAD, json!({"name": "Painting"})).is_err());
            assert!(s.execute(WORKSPACE_DELETE, json!({"name": "Painting"})).is_err());
            assert_eq!(s.prefs().clippy.layouts[LAYOUT_KEY], layout("layer"), "deleting keeps the current layout");
        }
    }

    /// A default layout (null) is a workspace too.
    #[test]
    fn the_default_layout_saves_as_null() {
        let mut s = Session::new();
        s.execute(LAYOUT_SET, json!({"layout": layout("layer")})).unwrap();
        s.execute(LAYOUT_RESET, json!({})).unwrap();
        s.execute(WORKSPACE_SAVE, json!({"name": "Default"})).unwrap();
        s.execute(LAYOUT_SET, json!({"layout": layout("layer")})).unwrap();
        assert_eq!(s.execute(WORKSPACE_LOAD, json!({"name": "Default"})).unwrap()["layout"], Value::Null);
        assert_eq!(s.prefs().clippy.layouts[LAYOUT_KEY], Value::Null);
    }

    #[test]
    fn bad_workspace_params_are_errors_and_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new();
        s.clippy.workspace_dir = Some(dir.path().to_path_buf());
        s.execute(LAYOUT_SET, json!({"layout": layout("layer")})).unwrap();
        s.execute(WORKSPACE_SAVE, json!({"name": "Keep"})).unwrap();
        let long = "x".repeat(MAX_WORKSPACE_NAME + 1);
        for id in [WORKSPACE_SAVE, WORKSPACE_LOAD, WORKSPACE_DELETE] {
            for p in [
                json!({}),
                Value::Null,
                json!([]),
                json!("Keep"),
                json!({"name": null}),
                json!({"name": 1}),
                json!({"name": ["Keep"]}),
                json!({"name": ""}),
                json!({"name": "   "}),
                json!({"name": long}),
                json!({"name": "../Keep"}),
                json!({"name": "a/b"}),
                json!({"name": "a\\b"}),
                json!({"name": ".hidden"}),
                json!({"name": "dot."}),
                json!({"name": "C:x"}),
                json!({"name": "a\u{0}b"}),
                json!({"name": "line\nbreak"}),
                json!({"name": "what?"}),
            ] {
                let err = s.execute(id, p.clone()).unwrap_err();
                assert!(matches!(err, EngineError::BadParams { .. }), "{id} {p}: {err}");
            }
        }
        assert!(matches!(s.execute(WORKSPACE_SAVE, json!({"name": "New", "overwrite": "yes"})), Err(EngineError::BadParams { .. })));
        assert_eq!(s.execute(WORKSPACE_LIST, json!({})).unwrap(), json!({"workspaces": ["Keep"]}));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "nothing written beside the folder");
    }

    /// Files a user or another program put in the folder: skipped or an error, never a panic.
    #[test]
    fn broken_workspace_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new();
        s.clippy.workspace_dir = Some(dir.path().to_path_buf());
        s.execute(LAYOUT_SET, json!({"layout": layout("layer")})).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        write("Text.json", "not json");
        write("Other.json", r#"{"layout": {}}"#);
        write("NoLayout.json", r#"{"format": "photocraft-clippy-workspace", "layout": 3}"#);
        write("Huge.json", &format!(r#"{{"format": "photocraft-clippy-workspace", "layout": {{"x": "{}"}}}}"#, "a".repeat(MAX_LAYOUT_BYTES + 8192)));
        write("Big.json", &format!(r#"{{"format": "photocraft-clippy-workspace", "layout": {{"x": "{}"}}}}"#, "a".repeat(MAX_LAYOUT_BYTES)));
        write(".hidden.json", "{}");
        write("readme.txt", "");
        std::fs::create_dir(dir.path().join("Folder.json")).unwrap();
        assert_eq!(s.execute(WORKSPACE_LIST, json!({})).unwrap(), json!({"workspaces": ["Big", "Huge", "NoLayout", "Other", "Text"]}));
        for name in ["Text", "Other", "NoLayout", "Huge", "Big", "Folder"] {
            assert!(s.execute(WORKSPACE_LOAD, json!({"name": name})).is_err(), "{name}");
            assert_eq!(s.prefs().clippy.layouts[LAYOUT_KEY], layout("layer"), "{name}");
        }
    }

    #[test]
    fn subtool_commands_are_listed_but_refused_without_the_interface() {
        let mut s = Session::new();
        for id in SUBTOOL_COMMANDS {
            assert!(crate::commands::find(id).is_some(), "{id}");
            for p in [json!({}), Value::Null, json!({"path": "/nonexistent.sut"}), json!([1])] {
                assert!(s.execute(id, p).is_err(), "{id}");
            }
        }
        assert_eq!(s.prefs().clippy.subtool, "");
    }

    #[test]
    fn photocraft_shows_the_commands_nowhere() {
        for c in specs() {
            assert!(c.menu.is_empty() && c.shortcut.is_none(), "{}", c.id);
            assert!(c.id.starts_with("clippy."), "{}", c.id);
        }
    }
}

#[cfg(test)]
mod stock_tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// A session working on a stock in a temporary folder, deleting into a trash next to it.
    fn session() -> (tempfile::TempDir, PathBuf, Session) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("stock");
        std::fs::create_dir(&root).unwrap();
        stock::TEST_TRASH.with(|t| *t.borrow_mut() = Some(dir.path().join("Trash")));
        let mut s = Session::new();
        s.execute("prefs.set", json!({"path": "clippy.stockDir", "value": root.to_str().unwrap()})).unwrap();
        (dir, root, s)
    }

    fn abr(dir: &Path, name: &str) -> PathBuf {
        use photocraft_psd::abr::{AbrSample, LegacyBrush, LegacyTip, write_v12};
        let tip = || AbrSample { id: String::new(), width: 6, height: 4, depth: 8, data: vec![255; 24] };
        // Sampled tips: legacy computed brushes have no name of their own ("9 px Round").
        let brushes = [
            LegacyBrush { name: "Hard Round".into(), spacing: 25, anti_alias: true, tip: LegacyTip::Sampled(tip()) },
            LegacyBrush { name: "Soft/Round".into(), spacing: 10, anti_alias: true, tip: LegacyTip::Sampled(tip()) },
        ];
        let path = dir.join(name);
        std::fs::write(&path, write_v12(2, &brushes, true).unwrap()).unwrap();
        path
    }

    fn subtools(s: &mut Session) -> Vec<String> {
        let list = s.execute(STOCK_LIST, json!({})).unwrap();
        let mut out = Vec::new();
        for t in list["tools"].as_array().unwrap() {
            for g in t["groups"].as_array().unwrap() {
                for st in g["subtools"].as_array().unwrap() {
                    out.push(format!("{}/{}/{}", t["dir"].as_str().unwrap(), g["dir"].as_str().unwrap(), st["name"].as_str().unwrap()));
                }
            }
        }
        out
    }

    #[test]
    fn the_stock_folder_is_the_preference_or_the_settings_folder() {
        // The only test touching the process-wide settings folder.
        set_settings_dir(None);
        let mut s = Session::new();
        assert!(stock_dir(&s).is_err());
        assert!(s.execute(STOCK_LIST, json!({})).is_err());
        assert!(s.execute(STOCK_TOOL_CREATE, json!({"name": "Pen"})).is_err());
        let dir = tempfile::tempdir().unwrap();
        set_settings_dir(Some(dir.path().to_owned()));
        assert_eq!(stock_dir(&s).unwrap(), (dir.path().join(DEFAULT_STOCK_FOLDER), true));
        // The default stock is created on first use, not by looking.
        let list = s.execute(STOCK_LIST, json!({})).unwrap();
        assert_eq!((list["default"].clone(), list["tools"].clone()), (json!(true), json!([])));
        assert!(!dir.path().join(DEFAULT_STOCK_FOLDER).exists());
        s.execute(STOCK_TOOL_CREATE, json!({"name": "Pen"})).unwrap();
        assert!(dir.path().join("stock/Pen").is_dir());
        set_settings_dir(None);
        // A chosen folder must be absolute and exist; it is never created.
        s.execute("prefs.set", json!({"path": "clippy.stockDir", "value": "relative/stock"})).unwrap();
        assert!(s.execute(STOCK_LIST, json!({})).is_err());
        let missing = dir.path().join("missing");
        s.execute("prefs.set", json!({"path": "clippy.stockDir", "value": missing.to_str().unwrap()})).unwrap();
        assert!(s.execute(STOCK_TOOL_CREATE, json!({"name": "Pen"})).is_err());
        assert!(s.execute(STOCK_LIST, json!({})).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn every_operation_works_through_commands() {
        let (dir, root, mut s) = session();
        assert_eq!(s.execute(STOCK_TOOL_CREATE, json!({"name": "Pen"})).unwrap(), json!({"path": "Pen", "kind": "tool"}));
        assert_eq!(s.execute(STOCK_GROUP_CREATE, json!({"tool": "Pen", "name": "Glatt"})).unwrap()["path"], "Pen/Glatt");
        // .abr → a new group named after the file in the brush tool (made, as there is none).
        let r = s.execute(STOCK_IMPORT, json!({"file": abr(dir.path(), "Legacy.abr").to_str().unwrap()})).unwrap();
        assert_eq!((r["kind"].as_str(), r["group"].as_str()), (Some("abr"), Some("Brush/Legacy")));
        assert_eq!(r["subtools"], json!(["Brush/Legacy/Hard Round", "Brush/Legacy/Soft_Round"]));
        assert_eq!(subtools(&mut s), ["Brush/Legacy/Hard Round", "Brush/Legacy/Soft_Round"]);
        // Move, copy, rename, sort, metadata.
        assert_eq!(s.execute(STOCK_MOVE, json!({"path": "Brush/Legacy/Hard Round", "to": "Pen/Glatt"})).unwrap()["path"], "Pen/Glatt/Hard Round");
        assert_eq!(s.execute(STOCK_COPY, json!({"path": "Pen/Glatt/Hard Round", "to": "Pen/Glatt"})).unwrap()["path"], "Pen/Glatt/Hard Round 2");
        assert_eq!(s.execute(STOCK_RENAME, json!({"path": "Pen/Glatt/Hard Round 2", "name": "Kopie"})).unwrap()["path"], "Pen/Glatt/Kopie");
        assert_eq!(s.execute(STOCK_SORT, json!({"path": "Pen/Glatt", "order": ["Kopie"]})).unwrap()["order"], json!(["Kopie.sut", "Hard Round.sut"]));
        assert_eq!(s.execute(STOCK_SORT, json!({"order": ["Pen", "Brush"]})).unwrap()["order"], json!(["Pen", "Brush"]));
        let m = s.execute(STOCK_META_SET, json!({"path": "Pen", "name": "Stift", "icon": "csp:3", "shortcut": "P"})).unwrap();
        assert_eq!(m, json!({"path": "Pen", "name": "Stift", "icon": "csp:3", "shortcut": "P"}));
        assert_eq!(s.execute(STOCK_META_SET, json!({"path": "Pen/Glatt/Kopie", "shortcut": "K"})).unwrap()["shortcut"], "K");
        let list = s.execute(STOCK_LIST, json!({})).unwrap();
        assert_eq!(list["root"], json!(root));
        assert_eq!(list["tools"][0]["name"], "Stift");
        assert_eq!(list["tools"][0]["groups"][0]["subtools"][0]["shortcut"], "K");
        assert_eq!(subtools(&mut s), ["Pen/Glatt/Kopie", "Pen/Glatt/Hard Round", "Brush/Legacy/Soft_Round"]);
        // Export, then import the exported .sut back.
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let e = s.execute(STOCK_EXPORT, json!({"path": "Pen/Glatt/Kopie", "to": out.to_str().unwrap()})).unwrap();
        assert_eq!(e["path"], json!(out.join("Kopie.sut")));
        let i = s.execute(STOCK_IMPORT, json!({"file": out.join("Kopie.sut").to_str().unwrap(), "group": "Brush/Legacy"})).unwrap();
        assert_eq!(i, json!({"kind": "sut", "subtools": ["Brush/Legacy/Kopie"]}));
        s.execute(STOCK_EXPORT, json!({"to": out.to_str().unwrap()})).unwrap();
        assert!(out.join("stock/Pen/Glatt/Kopie.sut").is_file());
        // Delete to the trash.
        assert_eq!(s.execute(STOCK_DELETE, json!({"path": "Brush"})).unwrap(), json!({"deleted": "Brush"}));
        assert!(dir.path().join("Trash/files/Brush/Legacy/Kopie.sut").is_file());
        assert_eq!(subtools(&mut s), ["Pen/Glatt/Kopie", "Pen/Glatt/Hard Round"]);
        // Save Brushes As: a copy, and the preference moves there.
        let new = dir.path().join("elsewhere");
        let r = s.execute(STOCK_SAVE_AS, json!({"to": new.to_str().unwrap()})).unwrap();
        assert_eq!(r["stockDir"], new.to_str().unwrap());
        assert_eq!(s.prefs().clippy.stock_dir, new.to_str().unwrap());
        s.execute(STOCK_DELETE, json!({"path": "Pen/Glatt/Kopie"})).unwrap();
        assert!(root.join("Pen/Glatt/Kopie.sut").is_file(), "the old place is unchanged");
        assert_eq!(subtools(&mut s), ["Pen/Glatt/Hard Round"]);
    }

    #[test]
    fn bad_params_are_errors_and_change_nothing() {
        let (dir, root, mut s) = session();
        s.execute(STOCK_TOOL_CREATE, json!({"name": "Pen"})).unwrap();
        s.execute(STOCK_GROUP_CREATE, json!({"tool": "Pen", "name": "Glatt"})).unwrap();
        let file = abr(dir.path(), "b.abr");
        s.execute(STOCK_IMPORT, json!({"file": file.to_str().unwrap(), "tool": "Pen"})).unwrap();
        let before = subtools(&mut s);
        let snapshot = |root: &Path| {
            let mut v = Vec::new();
            let mut stack = vec![root.to_owned()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    v.push(e.path());
                    if e.path().is_dir() {
                        stack.push(e.path());
                    }
                }
            }
            v.sort();
            v
        };
        let disk = snapshot(dir.path());
        let hostile = ["", "/", "..", "../x", "Pen/../..", "/etc/passwd", "Pen\\..", "a/b/c/d", ".hidden", "Pen/Glatt/x/y"];
        let mut cases: Vec<(&str, Value)> = Vec::new();
        for p in [json!({}), Value::Null, json!([]), json!("x"), json!(1)] {
            for cmd in [
                STOCK_TOOL_CREATE,
                STOCK_GROUP_CREATE,
                STOCK_RENAME,
                STOCK_DELETE,
                STOCK_MOVE,
                STOCK_COPY,
                STOCK_META_SET,
                STOCK_IMPORT,
                STOCK_EXPORT,
                STOCK_SAVE_AS,
            ] {
                cases.push((cmd, p.clone()));
            }
        }
        for h in hostile {
            cases.push((STOCK_TOOL_CREATE, json!({"name": h})));
            cases.push((STOCK_GROUP_CREATE, json!({"tool": "Pen", "name": h})));
            cases.push((STOCK_GROUP_CREATE, json!({"tool": h, "name": "G"})));
            cases.push((STOCK_RENAME, json!({"path": h, "name": "x"})));
            cases.push((STOCK_RENAME, json!({"path": "Pen", "name": h})));
            cases.push((STOCK_DELETE, json!({"path": h})));
            cases.push((STOCK_MOVE, json!({"path": h, "to": "Pen/Glatt"})));
            cases.push((STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": h})));
            cases.push((STOCK_COPY, json!({"path": "Pen/b/Hard Round", "to": h})));
            cases.push((STOCK_META_SET, json!({"path": h, "name": "x"})));
            if !h.is_empty() {
                // An empty folder path is the stock itself, valid for these two.
                cases.push((STOCK_SORT, json!({"path": h})));
                cases.push((STOCK_EXPORT, json!({"path": h, "to": dir.path().to_str().unwrap()})));
            }
            cases.push((STOCK_IMPORT, json!({"file": file.to_str().unwrap(), "tool": h})));
        }
        cases.extend([
            (STOCK_TOOL_CREATE, json!({"name": 5})),
            (STOCK_RENAME, json!({"path": "Pen", "name": null})),
            (STOCK_RENAME, json!({"path": "Missing", "name": "x"})),
            (STOCK_RENAME, json!({"path": "Pen/b/Soft_Round", "name": "Hard Round"})),
            (STOCK_DELETE, json!({"path": "Missing/Group"})),
            (STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": "Pen"})),
            (STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": "Pen/Glatt", "index": -1})),
            (STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": "Pen/Glatt", "index": 1.5})),
            (STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": "Pen/Glatt", "index": "0"})),
            (STOCK_MOVE, json!({"path": "Pen/b/Hard Round", "to": 3})),
            (STOCK_MOVE, json!({"path": "Pen", "to": "Pen/Glatt"})),
            (STOCK_COPY, json!({"path": "Pen/b", "to": "Pen/Glatt"})),
            (STOCK_SORT, json!({"path": "Pen/b", "order": "Hard Round"})),
            (STOCK_SORT, json!({"path": "Pen/b", "order": [1]})),
            (STOCK_SORT, json!({"path": "Pen/b", "order": ["../../x"]})),
            (STOCK_SORT, json!({"path": "Pen/b/Hard Round"})),
            (STOCK_META_SET, json!({"path": "Pen"})),
            (STOCK_META_SET, json!({"path": "Pen", "icon": 3})),
            (STOCK_META_SET, json!({"path": "Pen/b/Hard Round", "name": "x"})),
            (STOCK_IMPORT, json!({"file": ""})),
            (STOCK_IMPORT, json!({"file": "x\u{0}.sut"})),
            (STOCK_IMPORT, json!({"file": "a".repeat(5000)})),
            (STOCK_IMPORT, json!({"file": dir.path().join("missing.sut").to_str().unwrap(), "group": "Pen/Glatt"})),
            (STOCK_IMPORT, json!({"file": file.to_str().unwrap().replace(".abr", ".png")})),
            (STOCK_IMPORT, json!({"file": file.to_str().unwrap(), "name": 3})),
            (STOCK_IMPORT, json!({"file": "x.sut"})),
            (STOCK_IMPORT, json!({"file": "x.sut", "group": "Pen"})),
            (STOCK_IMPORT, json!({"file": "x.sut", "group": "../Pen"})),
            (STOCK_IMPORT, json!({"file": dir.path().join("missing.todb").to_str().unwrap()})),
            (STOCK_IMPORT, json!({"file": "x.todb", "materials": 1})),
            (STOCK_IMPORT, json!({"file": dir.path().join("missing.abr").to_str().unwrap()})),
            (STOCK_EXPORT, json!({"to": dir.path().join("missing").to_str().unwrap()})),
            (STOCK_EXPORT, json!({"to": root.join("Pen").to_str().unwrap()})),
            (STOCK_SAVE_AS, json!({"to": root.join("Pen/inside").to_str().unwrap()})),
            (STOCK_SAVE_AS, json!({"to": dir.path().to_str().unwrap()})),
            (STOCK_SAVE_AS, json!({"to": ""})),
        ]);
        let stock_dir = s.prefs().clippy.stock_dir.clone();
        for (cmd, p) in cases {
            assert!(s.execute(cmd, p.clone()).is_err(), "{cmd} {p}");
            assert_eq!(snapshot(dir.path()), disk, "{cmd} {p} changed the disk");
        }
        assert_eq!(subtools(&mut s), before);
        assert_eq!(s.prefs().clippy.stock_dir, stock_dir);
    }

    #[test]
    fn broken_files_are_listed_with_warnings() {
        let (_dir, root, mut s) = session();
        std::fs::create_dir_all(root.join("Pen/Glatt")).unwrap();
        std::fs::write(root.join("Pen/Glatt/Kaputt.sut"), b"not a sub tool").unwrap();
        std::fs::write(root.join("Pen/_clippy.json"), b"[").unwrap();
        let list = s.execute(STOCK_LIST, json!({})).unwrap();
        assert!(list["tools"][0]["groups"][0]["subtools"][0]["warning"].is_string());
        assert!(list["tools"][0]["warning"].is_string());
        assert_eq!(list["warnings"].as_array().unwrap().len(), 2);
    }
}
