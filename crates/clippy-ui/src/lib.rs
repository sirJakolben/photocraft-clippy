//! `photocraft-clippy-ui`: photocraft-clippy, a Clip Studio Paint–style interface on
//! top of PhotoCraft's. It plugs into `photocraft-ui-egui` as an
//! [`Extension`](photocraft_ui_egui::extension::Extension), which the desktop app installs with
//! [`install`] in its `clippy` feature; PhotoCraft without it is unchanged. Layer 6
//! (`xtask/src/layers.rs`). Every action goes through engine commands (`clippy.*`,
//! `photocraft_engine::clippy_cmds`); see `CLIPPY.md`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// Translate a string literal into the current UI language, as `photocraft-ui-egui`'s `tl!`. The
/// German strings live in its `i18n/de.tsv`, in the Clippy block at the end.
macro_rules! tl {
    ($s:expr) => {
        photocraft_ui_egui::i18n::t($s)
    };
}

pub mod dock;
pub mod mode;

use photocraft_ui_egui::extension::Extension;
use photocraft_ui_egui::{PhotocraftApp, Services, panels};

/// Clippy's interface as an extension of PhotoCraft's.
pub fn extension() -> Extension {
    Extension { active: mode::active, ui: draw, menu_items, pref_paths: &[mode::PREF_PATH], interface_prefs: mode::interface_prefs, invoke }
}

/// Install Clippy in the app's services, before the app is created.
pub fn install(services: &mut Services) {
    services.extension = Some(extension());
}

/// The Clippy interface: PhotoCraft's menu and status bars around Clippy's docks. Until the
/// tool property palette (M4, M9) PhotoCraft's option bar stays above the docks, so every tool
/// keeps its options. Without chrome (a screen mode that hides it) only the canvas shows, as in
/// PhotoCraft.
fn draw(app: &mut PhotocraftApp, ui: &mut egui::Ui, chrome: bool) {
    if !chrome {
        app.photocraft_chrome(ui, false);
        return;
    }
    if app.ui.panels.menu_bar {
        panels::title_bar(app, ui);
    }
    if app.ui.panels.options_bar {
        panels::options_bar(app, ui);
    }
    if app.ui.panels.status_bar {
        panels::status_bar(app, ui);
    }
    let t = photocraft_ui_egui::theme::Tokens::get(ui.ctx());
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| dock::show(app, ui));
}

/// Clippy's menu items: in Clippy mode its workspaces in Window › Workspace and its palettes in
/// Window; always the interface switch and the workspace reset, in Window › Interface.
fn menu_items(app: &PhotocraftApp, items: &mut Vec<photocraft_ui_egui::menus::MenuItem>) {
    dock::workspace::menu_items(app, items);
    mode::menu_items(app, items);
    dock::menu_items(app, items);
}

/// Clippy's own menu ids, in Clippy mode: a palette in Window, a workspace and the workspace
/// dialogs in Window › Workspace.
fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    if !mode::active(app) {
        return None;
    }
    if let Some(panel) = id.strip_prefix(dock::PALETTE_PREFIX) {
        let panel = serde_json::from_value::<dock::panels::PanelId>(serde_json::Value::String(panel.into()));
        return Some(panel.map_err(|_| format!("no palette `{id}`")).and_then(|p| dock::toggle_palette(app, ctx, p)));
    }
    dock::workspace::invoke(app, ctx, id, params)
}
