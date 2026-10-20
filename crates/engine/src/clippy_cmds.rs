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
    ]
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
    fn photocraft_shows_the_commands_nowhere() {
        for c in specs() {
            assert!(c.menu.is_empty() && c.shortcut.is_none(), "{}", c.id);
            assert!(c.id.starts_with("clippy."), "{}", c.id);
        }
    }
}
