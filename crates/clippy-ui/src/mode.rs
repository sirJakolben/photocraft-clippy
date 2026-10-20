//! The interface switch, Clippy ↔ Photoshop (features M1–M3): Window › Interface and
//! Preferences › Interface, both through the `clippy.mode.*` commands and the `clippy.mode`
//! preference. The frame reads the preference, so a switch shows at once and survives a restart.

use photocraft_engine::clippy_cmds::{ClippyMode, MODE_CLIPPY, MODE_PHOTOSHOP};
use photocraft_engine::prefs::Preferences;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::menus::{self, MenuItem};
use photocraft_ui_egui::theme::Tokens;
use serde_json::{Value, json};

/// The mode while the user hasn't chosen one: Clippy is the point of this build (feature M1).
pub const DEFAULT: ClippyMode = ClippyMode::Clippy;
/// The preference the dialog edits.
pub const PREF_PATH: &str = "clippy.mode";
/// Window › Interface. "Interface" is already translated (Preferences › Interface).
const MENU: [&str; 2] = ["Window", "Interface"];
/// The menu items: (command id, mode).
const ITEMS: [(&str, ClippyMode); 2] = [(MODE_CLIPPY, ClippyMode::Clippy), (MODE_PHOTOSHOP, ClippyMode::Photoshop)];

/// The mode in effect.
pub fn current(prefs: &Preferences) -> ClippyMode {
    prefs.clippy.mode.unwrap_or(DEFAULT)
}

/// The Photoshop mode is PhotoCraft's own interface, so the extension draws only in Clippy mode.
pub(crate) fn active(app: &PhotocraftApp) -> bool {
    current(app.session.prefs()) == ClippyMode::Clippy
}

/// Window › Interface › Clippy / Photoshop, the current one checked, after Window › Workspace.
pub(crate) fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    let mode = current(app.session.prefs());
    let rows = ITEMS.map(|(id, m)| MenuItem {
        id: id.into(),
        label: label(m).into(),
        path: MENU.map(String::from).to_vec(),
        shortcut: None,
        enabled: menus::is_enabled(app, id),
        checked: Some(m == mode),
        color: None,
    });
    let in_window = |i: &MenuItem| i.path.first().is_some_and(|p| p == "Window");
    let at = items
        .iter()
        .rposition(|i| in_window(i) && i.path.get(1).is_some_and(|p| p == "Workspace"))
        .map(|p| p + 1)
        .or_else(|| items.iter().position(in_window))
        .unwrap_or(items.len());
    items.splice(at..at, rows);
}

/// Preferences › Interface: a Clippy group with the mode. It edits the dialog's working copy, so
/// OK or Apply switches (through `prefs.set`) and Cancel keeps the mode in effect.
pub(crate) fn interface_prefs(ui: &mut egui::Ui, values: &mut Value, search: &str) {
    let title = tl!("Interface");
    let hint = tl!("Clippy: the Clip Studio Paint–style interface. Photoshop: PhotoCraft's own interface. Switches without a restart.");
    let words = [title, "clippy", "photoshop", "clip studio paint"];
    if !search.is_empty() && !words.iter().any(|w| w.to_lowercase().contains(search)) {
        return;
    }
    let shown = values.pointer("/clippy/mode").and_then(Value::as_str).and_then(ClippyMode::parse).unwrap_or(DEFAULT);
    let mut mode = shown;
    let t = Tokens::get(ui.ctx());
    egui::Frame::new().stroke(egui::Stroke::new(1.0, t.card_border)).corner_radius(t.radius).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("photocraft-clippy").strong().color(t.text));
        ui.add_space(4.0);
        egui::Grid::new("prefs-grid-clippy").num_columns(2).spacing([14.0, 7.0]).min_col_width(150.0).show(ui, |ui| {
            ui.label(egui::RichText::new(title).color(t.text_dim));
            let options = ClippyMode::ALL.map(|m| (m, label(m)));
            photocraft_ui_egui::widgets::dropdown(ui, "pref-clippy.mode", &mut mode, &options, 220.0);
            ui.end_row();
        });
        ui.label(egui::RichText::new(hint).color(t.text_faint));
    });
    ui.add_space(8.0);
    // Only a choice is written: an untouched default stays null, so the dialog sees no change.
    if mode != shown
        && let Some(clippy) = values.get_mut("clippy").and_then(Value::as_object_mut)
    {
        clippy.insert("mode".into(), json!(mode.id()));
    }
}

/// A mode's name in the menu and the dialog (product names, the same in every language).
fn label(mode: ClippyMode) -> &'static str {
    match mode {
        ClippyMode::Clippy => "Clippy",
        ClippyMode::Photoshop => "Photoshop",
    }
}
