//! Hooks for a build that adds its own interface on top of PhotoCraft's: Clippy (the desktop app's
//! `clippy` feature, crate `photocraft-clippy-ui`). The app binary puts an [`Extension`] in
//! [`crate::Services::extension`]; without one, as in PhotoCraft itself, nothing here runs and every
//! frame, menu and Preferences page is PhotoCraft's own. This crate never names the extension, so
//! it can depend on this one.

use serde_json::Value;

use crate::PhotocraftApp;
use crate::menus::MenuItem;

/// What an extension adds. Plain function pointers: the state lives in the session (preferences)
/// and in [`PhotocraftApp`], so the control channel and agents see it like any other state.
#[derive(Clone, Copy)]
pub struct Extension {
    /// Whether this frame draws the extension's interface instead of PhotoCraft's.
    pub active: fn(&PhotocraftApp) -> bool,
    /// Draws the window's chrome and canvas in place of PhotoCraft's panels (the
    /// `PhotocraftApp::photocraft_chrome` part of the frame). `chrome` is false while the screen
    /// mode hides the chrome, so only the canvas shows.
    pub ui: fn(&mut PhotocraftApp, &mut egui::Ui, bool),
    /// Adds the extension's items to the menu tree, before Edit › Menus hides or tints any.
    pub menu_items: fn(&PhotocraftApp, &mut Vec<MenuItem>),
    /// Preference paths outside PhotoCraft's sections that Edit › Preferences edits
    /// (`"clippy.mode"`): copied into the dialog's working copy and saved by its OK.
    pub pref_paths: &'static [&'static str],
    /// Draws the extension's rows on Preferences › Interface. `values` is the dialog's working
    /// copy (section → key → value, as PhotoCraft's sections); `search` is the lowercased
    /// filter, empty for every row.
    pub interface_prefs: fn(&mut egui::Ui, &mut Value, &str),
}

impl PhotocraftApp {
    /// The installed extension when it draws this frame.
    pub(crate) fn active_extension(&self) -> Option<Extension> {
        self.services.extension.filter(|e| (e.active)(self))
    }
}

/// Add the values of `extension`'s preference paths to a Preferences working copy.
pub(crate) fn add_pref_values(extension: Option<Extension>, prefs: &photocraft_engine::prefs::Preferences, values: &mut serde_json::Map<String, Value>) {
    for path in extension.map_or(&[][..], |e| e.pref_paths) {
        let Some((section, key)) = path.split_once('.') else { continue };
        let Some(value) = prefs.get(path) else { continue };
        let entry = values.entry(section.to_string()).or_insert_with(|| Value::Object(Default::default()));
        if let Value::Object(map) = entry {
            map.insert(key.to_string(), value);
        }
    }
}
