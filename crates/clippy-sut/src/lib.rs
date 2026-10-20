//! Reader and writer for Clip Studio Paint tool files, for Clippy (PhotoCraft Extension).
//!
//! - `.todb` (the whole tool tree of a Clip Studio Paint installation) and `.sut` (one exported sub
//!   tool with its brush tips) are SQLite databases with the same tables: [`Todb`], [`Sut`].
//! - Each sub tool's settings are one `Variant` row. Rows keep every column, known or not, with its
//!   storage class ([`Row`], [`Variant`]); [`known`] names the columns PhotoCraft maps.
//! - Blob columns with a known layout decode to [`Effector`] (pen pressure, tilt, velocity and
//!   random influence with their curves), [`ShowParams`] and [`MaterialRefs`]; unknown bytes are
//!   kept, so encoding a decoded blob gives the same bytes back.
//! - Brush tips are `.layer` files: [`layer::decode`] turns one into RGBA, [`layer::encode`] writes
//!   RGBA back.
//!
//! All inputs are treated as hostile: sizes are capped ([`Limits`]), the node tree is guarded against
//! cycles and depth, and every malformed file is an [`Error`], never a panic. SQLite and tar access
//! are desktop-only; the blob and `.layer` codecs also build for wasm.
//!
//! The file layouts are documented in this crate's README. Clip Studio Paint is a trademark of
//! CELSYS, Inc.; this crate is not affiliated with it.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
mod archive;
mod blobs;
mod bytes;
#[cfg(not(target_arch = "wasm32"))]
mod db;
mod effector;
pub mod known;
pub mod layer;
mod model;
#[cfg(all(feature = "synth", not(target_arch = "wasm32")))]
pub mod synth;
mod value;

#[cfg(not(target_arch = "wasm32"))]
pub use archive::{ArchiveEntry, MaterialArchive};
#[cfg(not(target_arch = "wasm32"))]
pub use db::write_atomic;
pub use blobs::{MaterialRef, MaterialRefs, ShowParams};
pub use effector::{Curve, Effector, Source, Sources};
pub use model::{DbInfo, Group, Material, SchemaObject, Sut, SubTool, Todb, Tool, ToolTree, Variant, is_null_uuid};
pub use value::{Cell, Row, Value};

/// Why a file could not be read or written. Messages are short reasons; callers add advice.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing the file failed.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite rejected the database (not a database, corrupt, or a failed statement).
    #[error("database error: {0}")]
    Sqlite(String),
    /// The file is not laid out as expected.
    #[error("malformed {what}: {reason}")]
    Malformed {
        /// The part of the file that is malformed.
        what: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// A size or count in the file is above a [`Limits`] cap.
    #[error("{what} is {value}, above the limit of {limit}")]
    Limit {
        /// What was counted.
        what: &'static str,
        /// The value found.
        value: u64,
        /// The cap.
        limit: u64,
    },
    /// A value has a layout this crate does not handle.
    #[error("unsupported {what}: {reason}")]
    Unsupported {
        /// The part of the file.
        what: &'static str,
        /// The layout found.
        reason: String,
    },
    /// The writer's template has no column for a value of the sub tool.
    #[error("the template table {table} has no column {column}")]
    UnknownColumn {
        /// Table name.
        table: String,
        /// Column name.
        column: String,
    },
    /// The operation needs SQLite, which the web build does not have.
    #[error("not supported in this build: {0}")]
    NotSupported(&'static str),
}

/// Shorthand for results of this crate.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn malformed<T>(what: &'static str, reason: impl Into<String>) -> Result<T> {
    Err(Error::Malformed { what, reason: reason.into() })
}

pub(crate) fn check_limit(what: &'static str, value: u64, limit: u64) -> Result<()> {
    if value > limit { Err(Error::Limit { what, value, limit }) } else { Ok(()) }
}

/// Caps applied while reading foreign files. The defaults are far above anything Clip Studio Paint
/// writes (the largest observed `.todb` is under 1 MB, the largest tip under 3 MB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest file read into memory.
    pub max_file_bytes: u64,
    /// Most nodes in a tool tree.
    pub max_nodes: u64,
    /// Most `Variant` or `MaterialFile` rows.
    pub max_rows: u64,
    /// Largest width or height of a decoded brush tip.
    pub max_tip_dimension: u32,
    /// Most pixels of a decoded brush tip.
    pub max_tip_pixels: u64,
    /// Largest decompressed size of one tile or one archive entry.
    pub max_entry_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1 << 30,
            max_nodes: 100_000,
            max_rows: 1_000_000,
            max_tip_dimension: 16_384,
            max_tip_pixels: 1 << 26,
            max_entry_bytes: 256 << 20,
        }
    }
}
