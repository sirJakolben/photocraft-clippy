//! The name, About text and project links the UI shows. PhotoCraft by default; a build derived
//! from it (Clippy, the desktop app's `clippy` feature) installs its own with [`set`] before the
//! first frame, so its window title, About dialog and Help menu are its own and never present it
//! as an ArtCraft product (`docs/brand/LICENSE-brand.txt`, "Forks and modified versions").

use std::sync::OnceLock;

/// What the UI calls the app and where its Help links lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Product name: window title, title bar, About dialog title.
    pub name: &'static str,
    /// The start screen's large title and the small word beside it.
    pub start_title: &'static str,
    pub start_subtitle: &'static str,
    /// The About dialog's one-line description (looked up in the UI translations).
    pub tagline: &'static str,
    /// Plain-text credit under the tagline; empty for none.
    pub based_on: &'static str,
    /// Help menu items to leave out (ids from `links::COMMANDS`).
    pub hidden_help: &'static [&'static str],
    /// Help items that open another URL than PhotoCraft's: (id, url).
    pub urls: &'static [(&'static str, &'static str)],
    /// Help items with another label than PhotoCraft's: (id, label).
    pub labels: &'static [(&'static str, &'static str)],
    /// The About dialog's links: (label, url). Empty keeps PhotoCraft's row and Discord button.
    pub about_links: &'static [(&'static str, &'static str)],
    /// The title bar's brand mark (128 px PNG); `None` keeps PhotoCraft's icon.
    pub icon_png: Option<&'static [u8]>,
}

/// PhotoCraft itself: nothing overridden.
pub const PHOTOCRAFT: Identity = Identity {
    name: "PhotoCraft",
    start_title: "PhotoCraft",
    start_subtitle: "open source",
    tagline: "PhotoCraft — an open-source, native image editor written in Rust.",
    based_on: "",
    hidden_help: &[],
    urls: &[],
    labels: &[],
    about_links: &[],
    icon_png: None,
};

/// Clippy's GitHub repository.
const CLIPPY_GITHUB: &str = "https://github.com/sirJakolben/photocraft-clippy";

/// Clippy (PhotoCraft Extension): a Clip Studio Paint–style interface on top of PhotoCraft.
pub const CLIPPY: Identity = Identity {
    name: "Clippy (PhotoCraft Extension)",
    start_title: "Clippy",
    start_subtitle: "PhotoCraft Extension",
    tagline: "Clippy (PhotoCraft Extension) — a Clip Studio Paint–style interface for PhotoCraft.",
    based_on: "Based on PhotoCraft by the ArtCraft team. Not affiliated with CELSYS or the ArtCraft team.",
    hidden_help: &["help.discord", "help.website", "help.artcraftWebsite"],
    urls: &[("help.github", CLIPPY_GITHUB), ("help.reportIssue", "https://github.com/sirJakolben/photocraft-clippy/issues")],
    labels: &[("help.github", "Clippy on GitHub"), ("help.about", "About Clippy")],
    about_links: &[("Clippy on GitHub", CLIPPY_GITHUB), ("PhotoCraft on GitHub", "https://github.com/storytold/photocraft")],
    icon_png: Some(include_bytes!("../../../assets/app-icon-clippy/hicolor/128x128/apps/io.github.sirjakolben.PhotocraftClippy.png")),
};

static IDENTITY: OnceLock<Identity> = OnceLock::new();

/// Install the identity for this process. `false` when one was installed already (it stays).
pub fn set(identity: Identity) -> bool {
    IDENTITY.set(identity).is_ok()
}

/// The identity for this process: the installed one, else [`PHOTOCRAFT`].
pub fn get() -> &'static Identity {
    IDENTITY.get().unwrap_or(&PHOTOCRAFT)
}

impl Identity {
    /// Whether the Help menu shows `id`.
    pub fn shows_help(&self, id: &str) -> bool {
        !self.hidden_help.contains(&id)
    }

    /// The URL a Help link opens instead of PhotoCraft's.
    pub fn url(&self, id: &str) -> Option<&'static str> {
        self.urls.iter().find(|(i, _)| *i == id).map(|(_, url)| *url)
    }

    /// The menu label `id` shows instead of PhotoCraft's.
    pub fn label(&self, id: &str) -> Option<&'static str> {
        self.labels.iter().find(|(i, _)| *i == id).map(|(_, label)| *label)
    }

    /// Whether this is PhotoCraft's own identity.
    pub fn is_photocraft(&self) -> bool {
        *self == PHOTOCRAFT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photocraft_is_the_default_and_overrides_nothing() {
        // Tests never install another identity, so PhotoCraft's tests see PhotoCraft.
        assert_eq!(get(), &PHOTOCRAFT);
        assert!(PHOTOCRAFT.is_photocraft());
        assert!(PHOTOCRAFT.shows_help("help.discord"));
        assert_eq!(PHOTOCRAFT.url("help.github"), None);
        assert_eq!(PHOTOCRAFT.label("help.about"), None);
    }

    #[test]
    fn clippy_links_to_itself_and_hides_artcraft_s_community_links() {
        assert!(!CLIPPY.is_photocraft());
        for id in ["help.discord", "help.website", "help.artcraftWebsite"] {
            assert!(!CLIPPY.shows_help(id), "{id}");
        }
        assert!(CLIPPY.shows_help("help.github") && CLIPPY.shows_help("help.reportIssue") && CLIPPY.shows_help("help.about"));
        assert!(CLIPPY.url("help.github").is_some_and(|u| u.contains("sirJakolben/photocraft-clippy")));
        assert!(CLIPPY.url("help.reportIssue").is_some_and(|u| u.ends_with("/issues")));
        assert!(!CLIPPY.name.contains("ArtCraft"));
        assert!(CLIPPY.about_links.iter().all(|(_, url)| !url.contains("artcraft")));
        let icon = CLIPPY.icon_png.map(photocraft_codecs::decode);
        assert!(icon.is_some_and(|i| i.is_ok_and(|i| i.width() == 128 && i.height() == 128)), "Clippy's title-bar icon decodes");
    }
}
