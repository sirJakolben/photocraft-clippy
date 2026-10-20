//! Changes Clippy makes itself, so the watcher doesn't report them as foreign (P6).
//!
//! Every write, rename, copy and deletion of this crate notes its paths here; for
//! [`OWN_WINDOW`] the watcher counts events on those paths (or inside them) as Clippy's own. Each
//! change also moves a revision ([`revision`]), so a [`crate::live::LiveStock`] reloads after an
//! operation another part of the program ran (an engine command), and moves are kept
//! ([`moves_since`]) so the selection follows them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long after a change events on its paths count as Clippy's own.
pub const OWN_WINDOW: Duration = Duration::from_secs(3);
/// Most paths remembered at once (older ones are forgotten first).
const MAX_NOTES: usize = 4096;
/// Most moves remembered.
const MAX_MOVES: usize = 256;

static REVISION: AtomicU64 = AtomicU64::new(0);
static NOTES: Mutex<Vec<(PathBuf, Instant)>> = Mutex::new(Vec::new());
static MOVES: Mutex<Vec<(u64, PathBuf, PathBuf)>> = Mutex::new(Vec::new());

/// Notes that Clippy is about to change `path` (a file or a folder with everything in it).
pub fn note(path: &Path) {
    let now = Instant::now();
    let mut notes = NOTES.lock().unwrap_or_else(PoisonError::into_inner);
    notes.retain(|(_, at)| now.duration_since(*at) < OWN_WINDOW);
    if notes.len() >= MAX_NOTES {
        notes.remove(0);
    }
    notes.push((path.to_owned(), now));
    REVISION.fetch_add(1, Ordering::SeqCst);
}

/// Notes a move or rename from `from` to `to` (both paths count as own).
pub fn note_move(from: &Path, to: &Path) {
    note(from);
    note(to);
    let rev = REVISION.load(Ordering::SeqCst);
    let mut moves = MOVES.lock().unwrap_or_else(PoisonError::into_inner);
    if moves.len() >= MAX_MOVES {
        moves.remove(0);
    }
    moves.push((rev, from.to_owned(), to.to_owned()));
}

/// Whether an event on `path` is one of Clippy's own changes: `path` or a folder holding it was
/// noted within [`OWN_WINDOW`].
pub fn is_own(path: &Path) -> bool {
    let now = Instant::now();
    let notes = NOTES.lock().unwrap_or_else(PoisonError::into_inner);
    notes.iter().any(|(p, at)| now.duration_since(*at) < OWN_WINDOW && path.starts_with(p))
}

/// Moves with every own change so far; a different value means something changed.
pub fn revision() -> u64 {
    REVISION.load(Ordering::SeqCst)
}

/// The moves noted after revision `rev`, oldest first.
pub fn moves_since(rev: u64) -> Vec<(PathBuf, PathBuf)> {
    let moves = MOVES.lock().unwrap_or_else(PoisonError::into_inner);
    moves.iter().filter(|(r, _, _)| *r > rev).map(|(_, a, b)| (a.clone(), b.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noted_paths_and_their_contents_are_own() {
        let rev = revision();
        let dir = PathBuf::from("/own-test/stock/Pen");
        note(&dir);
        assert!(is_own(&dir.join("Glatt/G-Pen.sut")));
        assert!(!is_own(Path::new("/own-test/stock/Brush")));
        assert!(revision() > rev);
        note_move(Path::new("/own-test/a"), Path::new("/own-test/b"));
        assert!(moves_since(rev).iter().any(|(a, b)| a == Path::new("/own-test/a") && b == Path::new("/own-test/b")));
    }
}
