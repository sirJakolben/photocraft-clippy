//! `photocraft-clippy-stock`: Clippy's sub tool stock on disk: the folders the sub tools live in,
//! their metadata, watching for changes, and import and export of Clip Studio Paint files.
//!
//! The stock is a folder anywhere the user likes (preference `clippy.stockDir`):
//!
//! ```text
//! <stock>/_clippy.json                    order of the tools
//! <stock>/<Tool>/_clippy.json             display name, icon, shortcut, order of the groups
//! <stock>/<Tool>/<Group>/_clippy.json     display name, icon, shortcut, order and shortcuts of the sub tools
//! <stock>/<Tool>/<Group>/<Sub tool>.sut   one sub tool, Clip Studio Paint's own format
//! ```
//!
//! What a file manager does there is what the program shows: [`Stock::load`] reads the folders as
//! they are, [`watch::StockWatcher`] follows them while the program runs and [`live::LiveStock`]
//! keeps a loaded stock and its selection up to date. Files the loader doesn't know are ignored, a
//! missing `_clippy.json` means folder names, the default icon and alphabetical order, and a
//! broken `.sut` is an entry with a warning; nothing in a stock folder can make loading fail
//! except the stock folder itself being unreadable.
//!
//! Every change Clippy makes goes through [`ops`], [`import`] and [`export`]: names are checked
//! ([`names`]), writes are atomic, and deletions go to the trash ([`trash`]). File-system code is
//! desktop-only (`cfg(not(target_arch = "wasm32"))`); the web build has the types and names only.
//! Layer 4 (`xtask/src/layers.rs`).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod meta;
pub mod model;
pub mod names;

#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(not(target_arch = "wasm32"))]
mod fsutil;
#[cfg(not(target_arch = "wasm32"))]
pub mod import;
#[cfg(not(target_arch = "wasm32"))]
pub mod live;
#[cfg(not(target_arch = "wasm32"))]
pub mod ops;
#[cfg(not(target_arch = "wasm32"))]
pub mod own;
#[cfg(not(target_arch = "wasm32"))]
mod template;
#[cfg(not(target_arch = "wasm32"))]
pub mod trash;
#[cfg(not(target_arch = "wasm32"))]
pub mod watch;

pub use meta::FolderMeta;
pub use model::{GroupEntry, Stock, SubToolEntry, ToolEntry};
pub use names::{EntryPath, META_FILE, SUT_EXT};
#[cfg(not(target_arch = "wasm32"))]
pub use template::{PRESET_COLUMN, stored_preset};

use std::path::{Path, PathBuf};

/// Why a stock operation failed. Messages name the file or entry and say what to do.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a file or folder failed.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The operating system's error.
        source: std::io::Error,
    },
    /// A sub tool or tool database could not be read or written.
    #[error("{}: {source}", path.display())]
    Sut {
        /// The file.
        path: PathBuf,
        /// What is wrong with it.
        source: photocraft_clippy_sut::Error,
    },
    /// A name can't be a tool, group or sub tool name (empty, `..`, a path separator …).
    #[error("`{name}` can't be used as a name: {reason}")]
    BadName {
        /// The name as given.
        name: String,
        /// Why not.
        reason: &'static str,
    },
    /// A stock path (`Tool/Group/Sub tool`) is malformed or names the wrong kind of entry.
    #[error("bad stock path `{path}`: {reason}")]
    BadPath {
        /// The path as given.
        path: String,
        /// Why not.
        reason: String,
    },
    /// The entry or folder does not exist.
    #[error("{0} does not exist")]
    NotFound(String),
    /// An entry of that name exists already.
    #[error("{0} exists already")]
    Exists(String),
    /// The operation is not possible in this state (exporting a folder into itself …).
    #[error("{0}")]
    Invalid(String),
    /// A size or count is above a cap of this crate.
    #[error("{what} is {value}, above the limit of {limit}")]
    Limit {
        /// What was counted.
        what: &'static str,
        /// The value found.
        value: u64,
        /// The cap.
        limit: u64,
    },
    /// Not available in this build or on this system.
    #[error("not supported here: {0}")]
    NotSupported(&'static str),
}

/// Shorthand for results of this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io { path: path.to_owned(), source }
    }

    pub(crate) fn sut(path: &Path, source: photocraft_clippy_sut::Error) -> Self {
        Error::Sut { path: path.to_owned(), source }
    }
}

pub(crate) fn check_limit(what: &'static str, value: u64, limit: u64) -> Result<()> {
    if value > limit { Err(Error::Limit { what, value, limit }) } else { Ok(()) }
}

/// Most entries read from one folder; the rest are skipped with a warning.
pub const MAX_FOLDER_ENTRIES: usize = 10_000;
/// Largest `.sut` read (the largest observed is a few MB).
pub const MAX_SUT_BYTES: u64 = 256 << 20;
/// Largest `_clippy.json` read.
pub const MAX_META_BYTES: u64 = 1 << 20;
