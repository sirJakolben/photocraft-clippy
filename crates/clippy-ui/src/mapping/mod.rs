//! Clip Studio Paint sub tools ↔ PhotoCraft (feature M8): which PhotoCraft tool a sub tool is
//! ([`tool`]), and how its settings (`Variant` columns) read into and write back from
//! PhotoCraft's brush and tool options ([`RULES`]).
//!
//! The mapping is a table of [`Rule`]s. Each rule owns a few columns and the PhotoCraft fields
//! they stand for, and converts both ways (units, ranges, the blend mode table, pen settings).
//! Writing is change-driven and minimal:
//!
//! - [`write_changed`] writes only the rules whose PhotoCraft fields changed since the settings
//!   were read ([`Rule::key`]), so every other column keeps its bytes;
//! - inside a rule a column is only written when it doesn't already read as the new value, so a
//!   value PhotoCraft can't hold exactly (a real size, a curve) stays as Clip Studio Paint wrote
//!   it until the user changes it;
//! - a column the file doesn't have is never added, and columns without a PhotoCraft counterpart
//!   are never touched (they stay in the `.sut`, not shown).
//!
//! Units and ranges of the columns are observations and assumptions (`CLIPPY.md`, "Mapping");
//! `tests/mapping.rs` checks them against real files when `CLIPPY_CSP_FIXTURES` is set.

pub mod blend;
mod brush;
mod cols;
pub mod effector;
mod options;
pub mod reverse;
pub mod tool;

use photocraft_clippy_sut::{Material, Variant};
use photocraft_engine::BrushSettings;
use photocraft_ui_egui::state::{Tool, ToolOptions};
use serde_json::Value;

pub use tool::{Choice, Extra};

/// The PhotoCraft side of a sub tool: what the mapping reads into and writes from. The shell's
/// live state is the working copy of the active sub tool (`crate::subtool`).
#[derive(Clone, Debug, PartialEq)]
pub struct Working {
    /// The tool's brush.
    pub brush: BrushSettings,
    /// The options bar's options (shared by the tools, as in PhotoCraft).
    pub options: ToolOptions,
    /// Selection tools: 0 new, 1 add, 2 subtract, 3 intersect (`UiState::selection_mode`).
    pub selection_mode: u8,
    /// View › Snap (`Extras::snap`): Clip Studio Paint's per-sub-tool "Enable snapping".
    pub snap: bool,
}

impl Default for Working {
    fn default() -> Self {
        Self { brush: BrushSettings::default(), options: ToolOptions::default(), selection_mode: 0, snap: true }
    }
}

/// What a conversion needs besides the columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ctx {
    /// Resolution for sizes stored in physical units (pixels per inch): the active document's.
    pub ppi: f64,
}

impl Default for Ctx {
    fn default() -> Self {
        Self { ppi: 72.0 }
    }
}

/// What a rule reads: the settings row, the materials embedded in the `.sut` (brush tips and
/// textures) and the context.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub variant: &'a Variant,
    pub materials: &'a [Material],
    pub ctx: Ctx,
}

/// Which tools a rule applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Brushes, pens, erasers, blending tools, the clone stamp.
    Brush,
    /// The Mixer Brush only (colour mixing).
    Mixer,
    /// The Blur tool only (its strength).
    Blur,
    /// Marquees, lassos and the auto select (magic wand).
    Selection,
    /// The auto select (magic wand) and the fill (paint bucket).
    Region,
    /// The fill (paint bucket).
    Fill,
    /// Text.
    Text,
    /// The eyedropper.
    Eyedropper,
    /// The move and object tools.
    Move,
}

impl Scope {
    /// Whether a sub tool mapped to `tool` uses rules of this scope.
    pub fn applies(self, tool: Tool) -> bool {
        match self {
            Scope::Brush => {
                matches!(tool, Tool::Brush | Tool::Pencil | Tool::MixerBrush | Tool::Eraser | Tool::Blur | Tool::Smudge | Tool::CloneStamp)
            }
            Scope::Mixer => tool == Tool::MixerBrush,
            Scope::Blur => tool == Tool::Blur,
            Scope::Selection => matches!(tool, Tool::RectMarquee | Tool::EllipseMarquee | Tool::Lasso | Tool::PolygonLasso | Tool::MagicWand),
            Scope::Region => matches!(tool, Tool::MagicWand | Tool::PaintBucket),
            Scope::Fill => tool == Tool::PaintBucket,
            Scope::Text => tool == Tool::Type,
            Scope::Eyedropper => tool == Tool::Eyedropper,
            Scope::Move => tool == Tool::Move,
        }
    }
}

/// One row of the mapping table: some columns ↔ some PhotoCraft fields.
pub struct Rule {
    /// Short id, reported by `clippy.subtool.get` and `clippy.subtool.writeBack`.
    pub id: &'static str,
    pub scope: Scope,
    /// The columns the rule reads (and may write); every other column is kept as it is.
    pub columns: &'static [&'static str],
    /// Read the columns into the working copy; a missing or unreadable column leaves its field.
    pub read: fn(&Source<'_>, &mut Working),
    /// Write the working copy into the columns the variant has.
    pub write: fn(&Working, &mut Variant, Ctx),
    /// The PhotoCraft fields the rule covers, for change detection: the rule is written back
    /// when its key changed. `Null` for read-only rules.
    pub key: fn(&Working) -> Value,
}

/// Every rule, brushes first.
pub fn rules() -> impl Iterator<Item = &'static Rule> {
    brush::RULES.iter().chain(options::RULES.iter())
}

/// The rules for a sub tool mapped to `tool`.
pub fn rules_for(tool: Tool) -> impl Iterator<Item = &'static Rule> {
    rules().filter(move |r| r.scope.applies(tool))
}

/// The working copy a sub tool mapped to `tool` reads as: `base` with every applying rule read
/// from `src`. Brushes start from `base.brush`; fields no rule covers keep their value.
pub fn read(src: &Source<'_>, tool: Tool, base: Working) -> Working {
    let mut w = base;
    for rule in rules_for(tool) {
        (rule.read)(src, &mut w);
    }
    if Scope::Brush.applies(tool) {
        brush::finish(&mut w.brush);
    }
    w
}

/// Write the rules whose fields differ between `before` and `now` into `variant`; returns their
/// ids. Nothing else in the row changes.
pub fn write_changed(tool: Tool, before: &Working, now: &Working, variant: &mut Variant, ctx: Ctx) -> Vec<&'static str> {
    let mut written = Vec::new();
    for rule in rules_for(tool) {
        let key = (rule.key)(now);
        if !key.is_null() && key != (rule.key)(before) {
            (rule.write)(now, variant, ctx);
            written.push(rule.id);
        }
    }
    written
}

/// Write every applying rule (a new `.sut`, or a check that a round trip keeps the values).
pub fn write_all(tool: Tool, now: &Working, variant: &mut Variant, ctx: Ctx) {
    for rule in rules_for(tool) {
        (rule.write)(now, variant, ctx);
    }
}

/// The columns of `variant` no rule for `tool` reads: kept in the `.sut`, not shown (the row's
/// id and Clip Studio Paint's palette state left out).
pub fn kept_columns(tool: Option<Tool>, variant: &Variant) -> Vec<String> {
    let mapped: Vec<&str> = tool.map(|t| rules_for(t).flat_map(|r| r.columns.iter().copied()).collect()).unwrap_or_default();
    variant
        .row
        .cells()
        .iter()
        .map(|c| c.name.as_str())
        .filter(|n| !mapped.iter().any(|m| m.eq_ignore_ascii_case(n)))
        .filter(|n| !matches!(*n, "VariantID" | photocraft_clippy_sut::known::SHOW_PARAM))
        .map(str::to_owned)
        .collect()
}

/// The ids of the rules for `tool` whose columns `variant` has.
pub fn mapped_rules(tool: Tool, variant: &Variant) -> Vec<&'static str> {
    rules_for(tool).filter(|r| r.columns.iter().any(|c| variant.row.contains(c))).map(|r| r.id).collect()
}
