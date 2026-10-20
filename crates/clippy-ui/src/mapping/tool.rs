//! Which PhotoCraft tool a sub tool is: its `NodeInputOp`/`NodeOutputOp` pair (how it takes input,
//! what it does), refined by a few settings and its name.
//!
//! | In/Out | Clip Studio Paint | PhotoCraft |
//! |---|---|---|
//! | 1/1, 2/2, 3/3 | zoom, hand, rotate | Zoom, Hand, Rotate View |
//! | 10/10 | brushes, airbrush, decoration, blend | Brush; colour mixing on: Mixer Brush; named blur: Blur; smudge: Smudge |
//! | 11/11 | pens, pencils | Brush; anti-aliasing 0: Pencil |
//! | 12/12 | eraser | Eraser |
//! | 13/13 | clone | Clone Stamp |
//! | 14/14 | dot pen | Brush with fixed spacing |
//! | 10/20 | selection pen | Brush in Quick Mask |
//! | 30/20, 31/20, 33/20 | rectangle/ellipse, lasso, polyline selection | Rect/Elliptical Marquee, Lasso, Polygonal Lasso |
//! | 50/20, 50/50 | auto select, fill | Magic Wand, Paint Bucket |
//! | 52/52 | gradient | Gradient |
//! | 40/40, 34/42, 45/45 | move layer, select layer, object | Move; with Auto-Select; with transform controls |
//! | 41/41 | eyedropper | Eyedropper |
//! | 30/30, 32/32 | rectangle/ellipse/polygon, line figure | Rectangle, Ellipse, Polygon, Line |
//! | 70/70 | text | Type |
//!
//! Everything else is [`Choice::Unsupported`]: lasso fill (31/31) and liquify (120/120) are still
//! to be built; vector, frame, balloon, ruler, correction and 3D tools have no PhotoCraft
//! counterpart.

use photocraft_clippy_sut::{SubTool, Variant, known};
use photocraft_ui_egui::state::Tool;

/// What PhotoCraft adds to the tool for a sub tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    None,
    /// The selection pen: paint the selection in Quick Mask.
    QuickMask,
    /// Select layer: the Move tool with Auto-Select (layers).
    AutoSelectLayer,
    /// Object: the Move tool with transform controls.
    ShowTransform,
    /// Dot pen: always spaced (no speed-driven spacing).
    FixedSpacing,
}

/// The PhotoCraft tool of a sub tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Tool {
        tool: Tool,
        extra: Extra,
    },
    /// No PhotoCraft counterpart: the sub tool can be selected, with this notice, and leaves
    /// PhotoCraft's tool as it is.
    Unsupported {
        reason: String,
    },
}

impl Choice {
    /// The PhotoCraft tool, if there is one.
    pub fn tool(&self) -> Option<Tool> {
        match self {
            Choice::Tool { tool, .. } => Some(*tool),
            Choice::Unsupported { .. } => None,
        }
    }

    fn plain(tool: Tool) -> Self {
        Choice::Tool { tool, extra: Extra::None }
    }

    fn with(tool: Tool, extra: Extra) -> Self {
        Choice::Tool { tool, extra }
    }

    fn unsupported(reason: impl Into<String>) -> Self {
        Choice::Unsupported { reason: reason.into() }
    }
}

/// Lowercased name parts that pick a tool among sub tools sharing an In/Out pair (Clip Studio
/// Paint's German, English and Japanese names).
const BLUR: [&str; 3] = ["weichzeichn", "blur", "ぼかし"];
const SMUDGE: [&str; 3] = ["verwisch", "smudge", "指先"];
const ELLIPSE: [&str; 4] = ["ellip", "oval", "kreis", "楕円"];
const POLYGON: [&str; 4] = ["polygon", "vieleck", "多角形", "polyline"];

fn named(name: &str, parts: &[&str]) -> bool {
    let name = name.to_lowercase();
    parts.iter().any(|p| name.contains(p))
}

/// Whether a blending brush mixes colours: some of the canvas colour comes into the paint
/// (amount of paint or its density below 100 %).
pub fn mixes_colour(v: &Variant) -> bool {
    let below = |c: &str| v.real(c).is_some_and(|x| x.is_finite() && x < 100.0);
    below(known::BRUSH_MIX_COLOR) || below(known::BRUSH_MIX_ALPHA)
}

/// The tool of `subtool` (its current settings refine pens and brushes).
pub fn choose_subtool(subtool: &SubTool) -> Choice {
    choose(subtool.input_op(), subtool.output_op(), subtool.name(), subtool.variant.as_ref())
}

/// The tool of a sub tool with In/Out `input`/`output`, named `name`, with settings `variant`.
pub fn choose(input: Option<i64>, output: Option<i64>, name: &str, variant: Option<&Variant>) -> Choice {
    let (Some(input), Some(output)) = (input, output) else {
        return Choice::unsupported("the sub tool doesn't say what kind of tool it is (no NodeInputOp/NodeOutputOp)");
    };
    match (input, output) {
        (1, 1) => Choice::plain(Tool::Zoom),
        (2, 2) => Choice::plain(Tool::Hand),
        (3, 3) => Choice::plain(Tool::RotateView),
        (10, 10) if named(name, &BLUR) => Choice::plain(Tool::Blur),
        (10, 10) if named(name, &SMUDGE) => Choice::plain(Tool::Smudge),
        (10, 10) if variant.is_some_and(mixes_colour) => Choice::plain(Tool::MixerBrush),
        (10, 10) => Choice::plain(Tool::Brush),
        (11, 11) if variant.and_then(|v| v.real(known::ANTI_ALIAS)) == Some(0.0) => Choice::plain(Tool::Pencil),
        (11, 11) => Choice::plain(Tool::Brush),
        (12, 12) => Choice::plain(Tool::Eraser),
        (13, 13) => Choice::plain(Tool::CloneStamp),
        (14, 14) => Choice::with(Tool::Brush, Extra::FixedSpacing),
        (10, 20) => Choice::with(Tool::Brush, Extra::QuickMask),
        (30, 20) if named(name, &ELLIPSE) => Choice::plain(Tool::EllipseMarquee),
        (30, 20) => Choice::plain(Tool::RectMarquee),
        (31, 20) => Choice::plain(Tool::Lasso),
        (33, 20) => Choice::plain(Tool::PolygonLasso),
        (50, 20) => Choice::plain(Tool::MagicWand),
        (50, 50) => Choice::plain(Tool::PaintBucket),
        (52, 52) => Choice::plain(Tool::Gradient),
        (40, 40) => Choice::plain(Tool::Move),
        (34, 42) => Choice::with(Tool::Move, Extra::AutoSelectLayer),
        (45, 45) => Choice::with(Tool::Move, Extra::ShowTransform),
        (41, 41) => Choice::plain(Tool::Eyedropper),
        (30, 30) if named(name, &ELLIPSE) => Choice::plain(Tool::EllipseShape),
        (30, 30) if named(name, &POLYGON) => Choice::plain(Tool::Polygon),
        (30, 30) => Choice::plain(Tool::Rectangle),
        (32, 32) => Choice::plain(Tool::Line),
        (70, 70) => Choice::plain(Tool::Type),
        (31, 31) => Choice::unsupported("lasso fill isn't available in PhotoCraft yet"),
        (120, 120) => Choice::unsupported("liquify as a sub tool isn't available yet; use Filter › Liquify"),
        _ => Choice::unsupported(format!("this kind of Clip Studio Paint tool (In/Out {input}/{output}) has no PhotoCraft counterpart")),
    }
}

/// The In/Out pair a new sub tool for `tool` gets (the reverse direction), `None` when Clip
/// Studio Paint has no such tool.
pub fn ops_of(tool: Tool) -> Option<(i64, i64)> {
    Some(match tool {
        Tool::Zoom => (1, 1),
        Tool::Hand => (2, 2),
        Tool::RotateView => (3, 3),
        Tool::Brush | Tool::MixerBrush | Tool::Blur | Tool::Smudge => (10, 10),
        Tool::Pencil => (11, 11),
        Tool::Eraser => (12, 12),
        Tool::CloneStamp => (13, 13),
        Tool::RectMarquee | Tool::EllipseMarquee => (30, 20),
        Tool::Lasso => (31, 20),
        Tool::PolygonLasso => (33, 20),
        Tool::MagicWand => (50, 20),
        Tool::PaintBucket => (50, 50),
        Tool::Gradient => (52, 52),
        Tool::Move => (40, 40),
        Tool::Eyedropper => (41, 41),
        Tool::Rectangle | Tool::EllipseShape | Tool::Polygon => (30, 30),
        Tool::Line => (32, 32),
        Tool::Type => (70, 70),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_clippy_sut::Value;

    fn variant(cols: &[(&str, Value)]) -> Variant {
        let mut v = Variant::default();
        for (c, x) in cols {
            v.set(c, x.clone());
        }
        v
    }

    #[test]
    fn pairs_and_refinements() {
        let pick = |i, o, name: &str, v: Option<&Variant>| choose(Some(i), Some(o), name, v).tool();
        assert_eq!(pick(11, 11, "G-Pen", None), Some(Tool::Brush));
        let aliased = variant(&[(known::ANTI_ALIAS, Value::Integer(0))]);
        assert_eq!(pick(11, 11, "Hard pencil", Some(&aliased)), Some(Tool::Pencil));
        assert_eq!(pick(10, 10, "Weichzeichnen", None), Some(Tool::Blur));
        assert_eq!(pick(10, 10, "Verwischen", None), Some(Tool::Smudge));
        assert_eq!(pick(10, 10, "Finger SMUDGE", None), Some(Tool::Smudge));
        let mixing = variant(&[(known::BRUSH_MIX_COLOR, Value::Integer(60)), (known::BRUSH_MIX_ALPHA, Value::Integer(100))]);
        let plain = variant(&[(known::BRUSH_MIX_COLOR, Value::Integer(100)), (known::BRUSH_MIX_ALPHA, Value::Integer(100))]);
        assert_eq!(pick(10, 10, "Oil paint", Some(&mixing)), Some(Tool::MixerBrush));
        assert_eq!(pick(10, 10, "Airbrush", Some(&plain)), Some(Tool::Brush));
        assert_eq!(pick(30, 20, "Ellipse", None), Some(Tool::EllipseMarquee));
        assert_eq!(pick(30, 20, "Rectangle", None), Some(Tool::RectMarquee));
        assert_eq!(pick(30, 30, "Polygon", None), Some(Tool::Polygon));
        assert_eq!(pick(30, 30, "Ellipse", None), Some(Tool::EllipseShape));
        assert_eq!(pick(34, 42, "Select layer", None), Some(Tool::Move));
        assert_eq!(choose(Some(10), Some(20), "Selection pen", None), Choice::Tool { tool: Tool::Brush, extra: Extra::QuickMask });
        assert_eq!(choose(Some(14), Some(14), "Dot pen", None), Choice::Tool { tool: Tool::Brush, extra: Extra::FixedSpacing });
    }

    #[test]
    fn large_and_unknown_tools_are_unsupported() {
        for (i, o) in [(31, 51), (10, 51), (50, 54), (30, 53), (10, 53), (60, 60), (90, 90), (80, 80), (100, 100), (110, 110), (31, 31), (120, 120), (-1, 7)] {
            assert!(matches!(choose(Some(i), Some(o), "x", None), Choice::Unsupported { .. }), "{i}/{o}");
        }
        assert!(matches!(choose(None, Some(10), "x", None), Choice::Unsupported { .. }));
    }

    #[test]
    fn every_reverse_pair_maps_back_to_a_tool() {
        for tool in Tool::ALL {
            if let Some((i, o)) = ops_of(tool) {
                assert!(choose(Some(i), Some(o), "", None).tool().is_some(), "{tool:?}");
            }
        }
    }
}
