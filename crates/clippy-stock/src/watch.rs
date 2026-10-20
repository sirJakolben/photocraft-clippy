//! Watching the stock folder (P6): what a file manager does there shows in the program without a
//! restart.
//!
//! [`StockWatcher`] follows the folder recursively (`notify`: inotify, FSEvents,
//! ReadDirectoryChangesW) and hands out the events in batches ([`BATCH_WINDOW`] after the first
//! one), so a folder copy with hundreds of files is one reload. Events on paths Clippy changed
//! itself ([`crate::own`]) are counted, not reported as foreign. Hidden files (temporary files of
//! atomic saves, import staging) are ignored, except that a hidden file renamed to a visible name
//! is that file appearing.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{Error, Result};

/// How long events are collected before they are handed out together.
pub const BATCH_WINDOW: Duration = Duration::from_millis(200);
/// Most changes kept in one batch; beyond it the batch asks for a full reload instead.
pub const MAX_BATCH: usize = 10_000;

/// One change on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A file or folder appeared.
    Created(PathBuf),
    /// A file or folder went away.
    Removed(PathBuf),
    /// A file's content or metadata changed.
    Modified(PathBuf),
    /// A file or folder was renamed or moved within the stock.
    Moved {
        /// Old path.
        from: PathBuf,
        /// New path.
        to: PathBuf,
    },
    /// Events were lost or the watched folder itself changed: reload everything.
    Rescan,
}

/// The changes of one batch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    /// Changes made by something other than Clippy, in the order seen.
    pub foreign: Vec<Change>,
    /// Number of events that were Clippy's own changes.
    pub own: usize,
}

impl Batch {
    /// Whether nothing happened.
    pub fn is_empty(&self) -> bool {
        self.foreign.is_empty() && self.own == 0
    }
}

/// Follows one stock folder.
pub struct StockWatcher {
    root: PathBuf,
    /// Events with whether they were Clippy's own, decided when they arrived.
    rx: Receiver<(notify::Result<Event>, bool)>,
    _watcher: RecommendedWatcher,
    pending: Batch,
    since: Option<Instant>,
}

impl std::fmt::Debug for StockWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StockWatcher").field("root", &self.root).field("pending", &self.pending).finish_non_exhaustive()
    }
}

impl StockWatcher {
    /// Starts watching `root` and everything below it.
    pub fn new(root: &Path) -> Result<Self> {
        let (tx, rx) = channel();
        // Whether an event is Clippy's own is decided on the watcher's thread as it arrives, so a
        // program that polls rarely still sees its own changes as its own.
        let handler = move |event: notify::Result<Event>| {
            let own = event.as_ref().is_ok_and(|e| !e.paths.is_empty() && e.paths.iter().any(|p| crate::own::is_own(p)));
            let _ = tx.send((event, own));
        };
        let mut watcher = notify::recommended_watcher(handler).map_err(|e| watch_error(root, e))?;
        watcher.watch(root, RecursiveMode::Recursive).map_err(|e| watch_error(root, e))?;
        Ok(Self { root: root.to_owned(), rx, _watcher: watcher, pending: Batch::default(), since: None })
    }

    /// The watched folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Takes the events that arrived; returns a batch once [`BATCH_WINDOW`] has passed since its
    /// first event. Never blocks.
    pub fn poll(&mut self) -> Option<Batch> {
        while let Ok((event, own)) = self.rx.try_recv() {
            self.take(event, own);
        }
        self.ready()
    }

    /// Waits at most `timeout` for a batch.
    pub fn wait(&mut self, timeout: Duration) -> Option<Batch> {
        let end = Instant::now() + timeout;
        loop {
            if let Some(batch) = self.poll() {
                return Some(batch);
            }
            let now = Instant::now();
            if now >= end {
                return None;
            }
            let due = self.since.map_or(end, |s| (s + BATCH_WINDOW).min(end));
            match self.rx.recv_timeout(due.saturating_duration_since(now).max(Duration::from_millis(1))) {
                Ok((event, own)) => self.take(event, own),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return self.ready().or_else(|| Some(Batch { foreign: vec![Change::Rescan], own: 0 })),
            }
        }
    }

    fn ready(&mut self) -> Option<Batch> {
        let since = self.since?;
        if since.elapsed() < BATCH_WINDOW {
            return None;
        }
        self.since = None;
        let batch = std::mem::take(&mut self.pending);
        (!batch.is_empty()).then_some(batch)
    }

    fn take(&mut self, event: notify::Result<Event>, own: bool) {
        self.since.get_or_insert_with(Instant::now);
        let change = match event {
            Ok(e) => self.classify(e),
            Err(_) => Some(Change::Rescan),
        };
        let Some(change) = change else { return };
        if own {
            self.pending.own += 1;
        } else if self.pending.foreign.len() < MAX_BATCH {
            if !self.pending.foreign.contains(&change) {
                self.pending.foreign.push(change);
            }
        } else if self.pending.foreign.last() != Some(&Change::Rescan) {
            self.pending.foreign.push(Change::Rescan);
        }
    }

    /// The change an event means; `None` to ignore it.
    fn classify(&self, e: Event) -> Option<Change> {
        if e.need_rescan() {
            return Some(Change::Rescan);
        }
        let visible = |p: &PathBuf| self.visible(p);
        let first = e.paths.first().cloned();
        let change = match e.kind {
            EventKind::Access(_) => return None,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => match (e.paths.first(), e.paths.get(1)) {
                (Some(from), Some(to)) => match (visible(from), visible(to)) {
                    (true, true) => Change::Moved { from: from.clone(), to: to.clone() },
                    (false, true) => Change::Created(to.clone()),
                    (true, false) => Change::Removed(from.clone()),
                    (false, false) => return None,
                },
                _ => Change::Rescan,
            },
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => Change::Removed(first.filter(visible)?),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => Change::Created(first.filter(visible)?),
            EventKind::Create(_) => Change::Created(first.filter(visible)?),
            EventKind::Remove(_) => {
                let path = first?;
                if path == self.root { Change::Rescan } else { Change::Removed(Some(path).filter(visible)?) }
            }
            EventKind::Modify(_) | EventKind::Any | EventKind::Other => match first {
                Some(p) if visible(&p) => Change::Modified(p),
                Some(_) => return None,
                None => Change::Rescan,
            },
        };
        Some(change)
    }

    /// Whether a path is something the stock shows: inside the root, no hidden part.
    fn visible(&self, path: &Path) -> bool {
        match path.strip_prefix(&self.root) {
            Ok(rel) => rel.components().all(|c| !c.as_os_str().to_string_lossy().starts_with('.')),
            Err(_) => false,
        }
    }
}

fn watch_error(root: &Path, e: notify::Error) -> Error {
    Error::io(root, std::io::Error::other(format!("can't watch the folder: {e}")))
}
