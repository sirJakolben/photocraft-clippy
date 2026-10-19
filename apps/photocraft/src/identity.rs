//! Who this build is: PhotoCraft, or Clippy (PhotoCraft Extension) when built with the `clippy`
//! feature (`cargo build -p photocraft --features clippy --bin photocraft-clippy`).
//!
//! The two must run side by side for one user, so everything that would make them share state or
//! mistake one for the other comes from here: the app id (Wayland/X11 window class, `.desktop`
//! name, icon name), the window title, the settings directory (and with it the single-instance
//! lock, the logs and crash recovery), the window icon and the executable an XWayland relaunch
//! starts. Clippy also tells the UI its name and links ([`photocraft_ui_egui::identity`]).

/// Matches the `.desktop` file and hicolor icon name, so Wayland docks pick up the icon.
#[cfg(not(feature = "clippy"))]
pub const APP_ID: &str = "ai.storyteller.photocraft";
#[cfg(feature = "clippy")]
pub const APP_ID: &str = "io.github.sirjakolben.PhotocraftClippy";

/// The product name in window titles and the eframe app name.
#[cfg(not(feature = "clippy"))]
pub const NAME: &str = "PhotoCraft";
#[cfg(feature = "clippy")]
pub const NAME: &str = "Clippy (PhotoCraft Extension)";

/// The executable on the PATH (an XWayland relaunch outside Flatpak and AppImage starts it).
#[cfg(not(feature = "clippy"))]
pub const BIN: &str = "photocraft";
#[cfg(feature = "clippy")]
pub const BIN: &str = "photocraft-clippy";

/// The settings folder under the platform's per-user config directory: (macOS and Windows, Linux).
#[cfg(not(feature = "clippy"))]
pub const CONFIG_DIRS: (&str, &str) = ("Photocraft", "photocraft");
#[cfg(feature = "clippy")]
pub const CONFIG_DIRS: (&str, &str) = ("PhotocraftClippy", "photocraft-clippy");

/// Tell the UI who it is. PhotoCraft keeps the UI's built-in identity; Clippy replaces it before
/// the first frame.
pub fn install() {
    #[cfg(feature = "clippy")]
    if !photocraft_ui_egui::identity::set(photocraft_ui_egui::identity::CLIPPY) {
        log::warn!("identity: the UI identity was already set");
    }
}

/// Clippy's first start: copy PhotoCraft's preferences (shortcuts, tool options, interface) so the
/// person's setup carries over; from then on the two keep their own. Only when Clippy has no
/// preferences yet and PhotoCraft has some. Best effort: a failure is logged and Clippy starts with
/// defaults.
#[cfg(feature = "clippy")]
pub fn adopt_photocraft_preferences(clippy_dir: &std::path::Path, env: &impl Fn(&str) -> Option<std::ffi::OsString>) {
    const PREFS: &str = "preferences.json";
    let target = clippy_dir.join(PREFS);
    if target.exists() {
        return;
    }
    let Some(source) = photocraft_dir(env).map(|d| d.join(PREFS)).filter(|p| p.is_file()) else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(clippy_dir).and_then(|()| std::fs::copy(&source, &target)) {
        log::warn!("identity: could not copy PhotoCraft's preferences from {}: {e}", source.display());
    } else {
        log::info!("identity: copied PhotoCraft's preferences from {}", source.display());
    }
}

/// PhotoCraft's own settings directory (the platform convention only, never portable mode).
#[cfg(feature = "clippy")]
fn photocraft_dir(env: &impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    crate::app_dirs::platform_dir_named(env, ("Photocraft", "photocraft"))
}

#[cfg(test)]
#[cfg(feature = "clippy")]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("photocraft-clippy-identity-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn clippy_never_shares_photocraft_s_identity() {
        assert_ne!(APP_ID, "ai.storyteller.photocraft");
        assert_ne!(BIN, "photocraft");
        assert_ne!(CONFIG_DIRS, ("Photocraft", "photocraft"));
    }

    #[test]
    fn the_first_start_copies_photocraft_s_preferences_once() {
        let root = temp("adopt");
        let config = root.join("config");
        std::fs::create_dir_all(config.join("photocraft")).unwrap();
        std::fs::write(config.join("photocraft/preferences.json"), "{\"a\":1}").unwrap();
        let env = |k: &str| (k == "XDG_CONFIG_HOME").then(|| config.clone().into_os_string());
        let clippy = config.join("photocraft-clippy");
        adopt_photocraft_preferences(&clippy, &env);
        assert_eq!(std::fs::read_to_string(clippy.join("preferences.json")).unwrap(), "{\"a\":1}");
        // Clippy's own preferences are never overwritten afterwards.
        std::fs::write(clippy.join("preferences.json"), "{\"b\":2}").unwrap();
        adopt_photocraft_preferences(&clippy, &env);
        assert_eq!(std::fs::read_to_string(clippy.join("preferences.json")).unwrap(), "{\"b\":2}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_to_copy_leaves_clippy_empty() {
        let root = temp("none");
        let env = |k: &str| (k == "XDG_CONFIG_HOME").then(|| root.clone().into_os_string());
        let clippy = root.join("photocraft-clippy");
        adopt_photocraft_preferences(&clippy, &env);
        assert!(!clippy.join("preferences.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
