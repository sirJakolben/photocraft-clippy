//! `photocraft-clippy-ui`: Clippy (PhotoCraft Extension), a Clip Studio Paint–style interface on
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

pub mod mode;

use photocraft_ui_egui::extension::Extension;
use photocraft_ui_egui::{PhotocraftApp, Services};

/// Clippy's interface as an extension of PhotoCraft's.
pub fn extension() -> Extension {
    Extension { active: mode::active, ui: draw, menu_items: mode::menu_items, pref_paths: &[mode::PREF_PATH], interface_prefs: mode::interface_prefs }
}

/// Install Clippy in the app's services, before the app is created.
pub fn install(services: &mut Services) {
    services.extension = Some(extension());
}

/// The Clippy interface. Until its docks (M2) it draws PhotoCraft's panels, so the app stays
/// usable in either mode.
fn draw(app: &mut PhotocraftApp, ui: &mut egui::Ui, chrome: bool) {
    app.photocraft_chrome(ui, chrome);
}
