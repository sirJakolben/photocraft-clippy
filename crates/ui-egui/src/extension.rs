//! Hooks for a build that adds its own interface on top of PhotoCraft's: Clippy (the desktop app's
//! `clippy` feature, crate `photocraft-clippy-ui`). The app binary puts an [`Extension`] in
//! [`crate::Services::extension`]; without one, as in PhotoCraft itself, nothing here runs and every
//! frame, menu and Preferences page is PhotoCraft's own. This crate never names the extension, so
//! it can depend on this one.

use serde_json::Value;

use crate::PhotocraftApp;
use crate::dock::Group;
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
    /// Carries out a command the extension owns (`clippy.*`): `Some(result)` when it handled
    /// `id`, `None` to leave it to PhotoCraft. Asked first for every command
    /// [`PhotocraftApp::run`] runs, in either interface, so the menu, shortcuts, the control
    /// channel and MCP reach it alike.
    pub command: fn(&mut PhotocraftApp, &str, &Value) -> Option<Result<Value, String>>,
    /// Runs once per frame after the panels, in either interface (beside the tool memory).
    pub frame: fn(&mut PhotocraftApp, &egui::Context),
    /// Hide PhotoCraft's brush presets (the Brush Preset picker's list) while the extension
    /// draws: its own tools bring their brushes.
    pub hides_brush_presets: bool,
}

impl PhotocraftApp {
    /// The installed extension when it draws this frame.
    pub(crate) fn active_extension(&self) -> Option<Extension> {
        self.services.extension.filter(|e| (e.active)(self))
    }

    /// The extension's own state of type `T` ([`PhotocraftApp::extension_data`]), made with
    /// `T::default()` the first time (or when another type is stored).
    pub fn extension_data<T: Default + Send + 'static>(&mut self) -> &mut T {
        if !self.extension_data.as_ref().is_some_and(|d| d.is::<T>()) {
            self.extension_data = Some(Box::new(T::default()));
        }
        #[allow(clippy::expect_used)]
        self.extension_data.as_mut().and_then(|d| d.downcast_mut::<T>()).expect("the slot holds a T: checked or filled just above")
    }
}

/// Select `tool` as the user's explicit choice (as the tool column does), switching its own brush
/// in at once (each tool keeps one), so the caller can set the brush right after.
pub fn select_tool(app: &mut PhotocraftApp, tool: crate::state::Tool) {
    crate::tool_memory::select(app, tool);
    app.ui.remember_group_tool();
    crate::paint_mouse::sync_tool_brush(app);
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

/// The body of PhotoCraft's dock panel `label` in `group` ("Layers" in [`Group::Layers`], …), for
/// an extension's own docks: scrolled as in PhotoCraft's dock. PhotoCraft's remembered tab of the
/// group is left as it was, so its own dock looks the same after a switch back. False (nothing
/// drawn) for a label the group doesn't have.
pub fn photocraft_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui, group: Group, label: &str) -> bool {
    let pro = crate::theme::Tokens::get(ui.ctx()).pro;
    let Some(tab) = group.tabs(pro).iter().position(|name| *name == label) else { return false };
    let remembered = app.ui.dock_tabs;
    *group.tab_mut(&mut app.ui.dock_tabs) = tab;
    if group.scrolls_itself(tab) {
        crate::panels::dock_body(app, ui, group, tab);
    } else {
        crate::panels::offer_color_field_fill(ui, pro, group, tab);
        egui::ScrollArea::vertical()
            .id_salt(("extension-panel-body", label))
            .auto_shrink([false, false])
            .show(ui, |ui| crate::panels::dock_body(app, ui, group, tab));
    }
    app.ui.dock_tabs = remembered;
    true
}
