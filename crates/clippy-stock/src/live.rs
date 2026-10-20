//! A loaded stock that stays up to date (P4, P6): [`LiveStock`] reloads when the watcher reports a
//! foreign change or when Clippy changed the stock itself ([`crate::own::revision`]), reading only
//! the files that changed, and keeps the selected sub tool selected when it is moved or renamed,
//! by Clippy or in a file manager.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::model::{LoadCache, Stock};
use crate::names::EntryPath;
use crate::watch::{Batch, Change, StockWatcher};
use crate::{Result, own};

/// What a [`LiveStock::poll`] applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    /// Changes made outside Clippy.
    pub foreign: Vec<Change>,
    /// Whether Clippy's own operations changed the stock.
    pub own: bool,
}

/// A stock with its watcher and selection.
#[derive(Debug)]
pub struct LiveStock {
    stock: Stock,
    cache: LoadCache,
    watcher: Option<StockWatcher>,
    watch_error: Option<String>,
    rev: u64,
    selected: Option<EntryPath>,
}

impl LiveStock {
    /// Loads `root` and starts watching it. A folder that can't be watched still loads; then
    /// [`LiveStock::watch_error`] says why and only Clippy's own changes show.
    pub fn open(root: &Path) -> Result<Self> {
        let rev = own::revision();
        let (watcher, watch_error) = match StockWatcher::new(root) {
            Ok(w) => (Some(w), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let mut cache = LoadCache::default();
        let stock = Stock::load_cached(root, &mut cache)?;
        Ok(Self { stock, cache, watcher, watch_error, rev, selected: None })
    }

    /// The stock as last loaded.
    pub fn stock(&self) -> &Stock {
        &self.stock
    }

    /// Why the folder isn't watched, if it isn't.
    pub fn watch_error(&self) -> Option<&str> {
        self.watch_error.as_deref()
    }

    /// The selected sub tool.
    pub fn selected(&self) -> Option<&EntryPath> {
        self.selected.as_ref()
    }

    /// Selects a sub tool (`None`: nothing).
    pub fn select(&mut self, path: Option<EntryPath>) {
        self.selected = path;
    }

    /// Reloads now, keeping the selection where it can.
    pub fn reload(&mut self) -> Result<()> {
        self.apply(&[])
    }

    /// Applies what changed since the last call, without waiting; `None` when nothing did.
    pub fn poll(&mut self) -> Result<Option<Update>> {
        let batch = self.watcher.as_mut().and_then(StockWatcher::poll);
        self.update(batch)
    }

    /// Like [`LiveStock::poll`], but waits up to `timeout` for a change.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<Update>> {
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let batch = match self.watcher.as_mut() {
                Some(w) => w.wait(left.min(Duration::from_millis(50))),
                None => {
                    std::thread::sleep(left.min(Duration::from_millis(50)));
                    None
                }
            };
            if let Some(update) = self.update(batch)? {
                return Ok(Some(update));
            }
            if Instant::now() >= end {
                return Ok(None);
            }
        }
    }

    fn update(&mut self, batch: Option<Batch>) -> Result<Option<Update>> {
        let rev = own::revision();
        let own_changed = rev != self.rev;
        let batch = batch.unwrap_or_default();
        if !own_changed && batch.foreign.is_empty() {
            return Ok(None);
        }
        // Moves Clippy made, then those seen on disk, in order.
        let mut moves: Vec<(std::path::PathBuf, std::path::PathBuf)> = if own_changed { own::moves_since(self.rev) } else { Vec::new() };
        moves.extend(batch.foreign.iter().filter_map(|c| match c {
            Change::Moved { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        }));
        self.rev = rev;
        self.apply(&moves)?;
        Ok(Some(Update { foreign: batch.foreign, own: own_changed }))
    }

    fn apply(&mut self, moves: &[(std::path::PathBuf, std::path::PathBuf)]) -> Result<()> {
        let root = self.stock.root.clone();
        let old_uuid = self.selected.as_ref().and_then(|s| self.stock.subtools().find(|(p, _)| p == s).map(|(_, e)| e.uuid.clone())).filter(|u| !u.is_empty());
        if let Some(sel) = self.selected.clone() {
            let mut disk = sel.to_path(&root);
            for (from, to) in moves {
                if let Ok(rest) = disk.strip_prefix(from) {
                    disk = to.join(rest);
                }
            }
            self.selected = EntryPath::from_disk(&root, &disk).or(Some(sel));
        }
        self.stock = Stock::load_cached(&root, &mut self.cache)?;
        if let Some(sel) = &self.selected
            && !self.stock.contains(sel)
        {
            // Not where it was: the same sub tool (by node UUID) if exactly one has it.
            let mut same = self.stock.subtools().filter(|(_, e)| old_uuid.as_deref().is_some_and(|u| e.uuid == u)).map(|(p, _)| p);
            self.selected = match (same.next(), same.next()) {
                (Some(p), None) => Some(p),
                _ => None,
            };
        }
        Ok(())
    }
}
