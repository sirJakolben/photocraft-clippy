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
pub mod mapping;
pub mod mode;
pub mod subtool;

use photocraft_ui_egui::extension::Extension;
use photocraft_ui_egui::{PhotocraftApp, Services, panels};

/// Clippy's interface as an extension of PhotoCraft's.
pub fn extension() -> Extension {
    Extension {
        active: mode::active,
        ui: draw,
        menu_items,
        pref_paths: &[mode::PREF_PATH],
        interface_prefs: mode::interface_prefs,
        command: subtool::command,
        frame: subtool::frame,
        hides_brush_presets: true,
    }
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

/// Clippy's menu items: the interface switch and the workspace reset, in Window › Interface.
fn menu_items(app: &PhotocraftApp, items: &mut Vec<photocraft_ui_egui::menus::MenuItem>) {
    mode::menu_items(app, items);
    dock::menu_items(app, items);
}
