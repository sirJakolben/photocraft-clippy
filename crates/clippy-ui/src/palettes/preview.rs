//! Stroke previews of the sub tool tiles (G13): each tile's brush is read from its `.sut` through
//! the mapping once, kept until the file changes (size or modification time), and drawn with
//! PhotoCraft's brush engine by `photocraft_ui_egui::brush_preview`, which caches the rendered
//! stroke per tile and brush. A few files are read per frame at most, so opening a large group
//! doesn't stall a frame; the rest follow on the next frames.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use photocraft_engine::BrushSettings;

/// Files read per frame at most.
pub(crate) const READS_PER_FRAME: u32 = 3;

/// The brushes of the tiles, by `.sut` path.
#[derive(Default)]
pub(crate) struct Cache {
    entries: HashMap<PathBuf, Entry>,
    /// Reads left this frame.
    budget: u32,
    /// The frame `budget` is for.
    frame: u64,
}

struct Entry {
    stamp: Stamp,
    /// `None`: the sub tool draws no stroke (not a brush, or unreadable).
    brush: Option<Arc<BrushSettings>>,
}

type Stamp = (u64, Option<std::time::SystemTime>);

impl Cache {
    /// The brush the sub tool at `path` paints with, `None` when it has none. `Err(())` when it
    /// isn't read yet (this frame's reads are used up): ask again next frame.
    pub(crate) fn brush(&mut self, path: &Path, frame: u64) -> Result<Option<Arc<BrushSettings>>, ()> {
        if self.frame != frame {
            self.frame = frame;
            self.budget = READS_PER_FRAME;
        }
        let stamp = stamp(path);
        if let Some(e) = self.entries.get(path)
            && e.stamp == stamp
            && stamp.1.is_some()
        {
            return Ok(e.brush.clone());
        }
        if self.budget == 0 {
            return Err(());
        }
        self.budget -= 1;
        let brush = read(path).map(Arc::new);
        self.entries.insert(path.to_owned(), Entry { stamp, brush: brush.clone() });
        Ok(brush)
    }

    /// Forget files no longer shown, keeping at most `keep` entries.
    pub(crate) fn trim(&mut self, keep: usize) {
        if self.entries.len() > keep {
            self.entries.clear();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn stamp(path: &Path) -> Stamp {
    std::fs::metadata(path).map(|m| (m.len(), m.modified().ok())).unwrap_or((0, None))
}

#[cfg(target_arch = "wasm32")]
fn stamp(_: &Path) -> Stamp {
    (0, None)
}

/// The brush of the `.sut` at `path` as the mapping reads it.
#[cfg(not(target_arch = "wasm32"))]
fn read(path: &Path) -> Option<BrushSettings> {
    use crate::mapping::{self, Ctx, Scope, Source, Working};

    let sut = photocraft_clippy_sut::Sut::open(path).ok()?;
    let tool = crate::mapping::tool::choose_subtool(&sut.subtool).tool().filter(|t| Scope::Brush.applies(*t))?;
    let variant = sut.subtool.variant.as_ref()?;
    let src = Source { variant, materials: &sut.subtool.materials, ctx: Ctx::default() };
    Some(mapping::read(&src, tool, Working::default()).brush)
}

#[cfg(target_arch = "wasm32")]
fn read(_: &Path) -> Option<BrushSettings> {
    None
}

/// Paint `brush`'s stroke into `rect`, in `color`, cached under `slot`.
pub(crate) fn paint(ui: &egui::Ui, rect: egui::Rect, slot: &str, brush: &BrushSettings, color: egui::Color32) {
    let ppp = ui.ctx().pixels_per_point();
    let (w, h) = ((rect.width() * ppp).round().max(8.0) as u32, (rect.height() * ppp).round().max(8.0) as u32);
    let texture = photocraft_ui_egui::brush_preview::stroke_texture(ui.ctx(), slot, brush, w, h, color);
    ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}
