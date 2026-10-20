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
    /// Folder of the sub tool stock; empty for the default inside the settings folder (M7).
    pub stock_dir: String,
    /// The dock layout of each mode, by mode id (M2). JSON owned by `photocraft-clippy-ui`.
    pub layouts: Value,
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
    ]
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
    fn photocraft_shows_the_commands_nowhere() {
        for c in specs() {
            assert!(c.menu.is_empty() && c.shortcut.is_none(), "{}", c.id);
            assert!(c.id.starts_with("clippy."), "{}", c.id);
        }
    }
}
