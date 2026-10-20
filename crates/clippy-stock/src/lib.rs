//! `photocraft-clippy-stock`: Clippy's sub tool stock on disk: the folders the sub tools live in,
//! their metadata, watching for changes, and import and export of Clip Studio Paint files.
//!
//! Empty until M7, which builds it on `photocraft-clippy-sut` (the `.sut` format) and
//! `photocraft-io`. Layer 4 (`xtask/src/layers.rs`); file-system code stays behind
//! `cfg(not(target_arch = "wasm32"))`, where the web build reports "not supported".

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
