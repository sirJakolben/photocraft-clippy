//! Columns of the other tools ↔ `ToolOptions` (and the selection mode): selection, auto select,
//! fill, text, eyedropper, move, and the Blur tool's strength.
//!
//! Only `Opacity`, `AntiAlias` and `BrushBlur` are names seen in Clip Studio Paint files; the
//! other column names below are unverified until `tests/mapping.rs` runs on real files
//! (`CLIPPY_CSP_FIXTURES`). A name a file doesn't have maps nothing and is never written, so a
//! wrong guess leaves the option as PhotoCraft has it.

use photocraft_clippy_sut::known as k;
use serde_json::json;

use super::cols::{flag, int, num_in, percent, put_flag, put_int, put_num, put_percent, put_text};
use super::{Rule, Scope};

/// Unverified column names (see the module docs).
pub mod col {
    /// Selection: 0 new, 1 add, 2 subtract, 3 intersect.
    pub const SELECTION_MODE: &str = "SelectionMode";
    /// Auto select and fill: colour margin in percent.
    pub const COLOR_MARGIN: &str = "ColorMargin";
    /// Auto select and fill: only adjacent pixels.
    pub const FOLLOW_ADJACENT_PIXEL: &str = "FollowAdjacentPixel";
    /// Auto select and fill: 0 the editing layer, others more layers.
    pub const REFER_LAYER_TYPE: &str = "ReferLayerType";
    pub const FONT_NAME: &str = "FontName";
    pub const FONT_STYLE: &str = "FontStyleName";
    /// Points.
    pub const FONT_SIZE: &str = "FontSize";
    /// 0 left, 1 centre, 2 right.
    pub const TEXT_ALIGNMENT: &str = "TextAlignment";
    /// Eyedropper: 0 the layer, 1 the canvas.
    pub const PICKUP_TARGET: &str = "PickupTarget";
    /// Move: pick the layer under the pointer.
    pub const AUTO_SELECT_LAYER: &str = "AutoSelectLayer";
}

/// PhotoCraft's text anti-aliasing for Clip Studio Paint's steps (0 none … 3 strong).
const TYPE_AA: [(i64, &str); 5] = [(0, "none"), (1, "sharp"), (2, "smooth"), (3, "strong"), (1, "crisp")];
const ALIGN: [(i64, &str); 3] = [(0, "left"), (1, "center"), (2, "right")];

fn name_of(table: &[(i64, &'static str)], n: i64) -> Option<&'static str> {
    table.iter().find(|(m, _)| *m == n).map(|(_, s)| *s)
}

fn number_of(table: &[(i64, &str)], s: &str) -> Option<i64> {
    table.iter().find(|(_, t)| *t == s).map(|(n, _)| *n)
}

pub(super) const RULES: [Rule; 12] = [
    Rule {
        id: "blurStrength",
        scope: Scope::Blur,
        columns: &[k::BRUSH_BLUR],
        read: |s, w| {
            if let Some(p) = num_in(s.variant, k::BRUSH_BLUR, 0.0, 100.0) {
                w.options.strength = p;
            }
        },
        write: |w, v, _| put_num(v, k::BRUSH_BLUR, f64::from(w.options.strength.clamp(0.0, 100.0)), 1e-3),
        key: |w| json!(w.options.strength),
    },
    Rule {
        id: "selectionMode",
        scope: Scope::Selection,
        columns: &[col::SELECTION_MODE],
        read: |s, w| {
            if let Some(n) = int(s.variant, col::SELECTION_MODE).filter(|n| (0..=3).contains(n)) {
                w.selection_mode = n as u8;
            }
        },
        write: |w, v, _| put_int(v, col::SELECTION_MODE, i64::from(w.selection_mode.min(3))),
        key: |w| json!(w.selection_mode),
    },
    Rule {
        id: "selectionAntiAlias",
        scope: Scope::Selection,
        columns: &[k::ANTI_ALIAS],
        read: |s, w| {
            if let Some(on) = flag(s.variant, k::ANTI_ALIAS) {
                w.options.anti_alias = on;
            }
        },
        write: |w, v, _| put_flag(v, k::ANTI_ALIAS, w.options.anti_alias),
        key: |w| json!(w.options.anti_alias),
    },
    Rule {
        id: "tolerance",
        scope: Scope::Region,
        columns: &[col::COLOR_MARGIN],
        read: |s, w| {
            // Percent of the range; PhotoCraft's tolerance is in levels (0–255).
            if let Some(p) = percent(s.variant, col::COLOR_MARGIN) {
                w.options.tolerance = p * 255.0;
            }
        },
        write: |w, v, _| put_percent(v, col::COLOR_MARGIN, w.options.tolerance.clamp(0.0, 255.0) / 255.0),
        key: |w| json!(w.options.tolerance),
    },
    Rule {
        id: "contiguous",
        scope: Scope::Region,
        columns: &[col::FOLLOW_ADJACENT_PIXEL],
        read: |s, w| {
            if let Some(on) = flag(s.variant, col::FOLLOW_ADJACENT_PIXEL) {
                w.options.contiguous = on;
            }
        },
        write: |w, v, _| put_flag(v, col::FOLLOW_ADJACENT_PIXEL, w.options.contiguous),
        key: |w| json!(w.options.contiguous),
    },
    Rule {
        id: "sampleAllLayers",
        scope: Scope::Region,
        columns: &[col::REFER_LAYER_TYPE],
        read: |s, w| {
            if let Some(n) = int(s.variant, col::REFER_LAYER_TYPE) {
                w.options.sample_all_layers = n != 0;
            }
        },
        write: |w, v, _| {
            // Any "more layers" choice reads as on: only switching it off writes 0.
            if (int(v, col::REFER_LAYER_TYPE).is_some_and(|n| n != 0)) != w.options.sample_all_layers {
                put_int(v, col::REFER_LAYER_TYPE, i64::from(w.options.sample_all_layers));
            }
        },
        key: |w| json!(w.options.sample_all_layers),
    },
    Rule {
        id: "fillOpacity",
        scope: Scope::Fill,
        columns: &[k::OPACITY, k::ANTI_ALIAS],
        read: |s, w| {
            if let Some(p) = num_in(s.variant, k::OPACITY, 0.0, 100.0) {
                w.options.fill_opacity = p;
            }
            if let Some(on) = flag(s.variant, k::ANTI_ALIAS) {
                w.options.anti_alias = on;
            }
        },
        write: |w, v, _| {
            put_num(v, k::OPACITY, f64::from(w.options.fill_opacity.clamp(0.0, 100.0)), 1e-3);
            put_flag(v, k::ANTI_ALIAS, w.options.anti_alias);
        },
        key: |w| json!([w.options.fill_opacity, w.options.anti_alias]),
    },
    Rule {
        id: "font",
        scope: Scope::Text,
        columns: &[col::FONT_NAME, col::FONT_STYLE],
        read: |s, w| {
            if let Some(name) = s.variant.text(col::FONT_NAME).filter(|n| !n.is_empty() && n.len() <= 256) {
                w.options.type_font = name.to_owned();
            }
            if let Some(style) = s.variant.text(col::FONT_STYLE).filter(|n| !n.is_empty() && n.len() <= 256) {
                w.options.type_style = style.to_owned();
            }
        },
        write: |w, v, _| {
            put_text(v, col::FONT_NAME, &w.options.type_font);
            put_text(v, col::FONT_STYLE, &w.options.type_style);
        },
        key: |w| json!([w.options.type_font, w.options.type_style]),
    },
    Rule {
        id: "fontSize",
        scope: Scope::Text,
        columns: &[col::FONT_SIZE],
        read: |s, w| {
            if let Some(pt) = num_in(s.variant, col::FONT_SIZE, 0.1, 10_000.0) {
                w.options.type_size = pt;
            }
        },
        write: |w, v, _| put_num(v, col::FONT_SIZE, f64::from(w.options.type_size), 1e-3),
        key: |w| json!(w.options.type_size),
    },
    Rule {
        id: "textAntiAlias",
        scope: Scope::Text,
        columns: &[k::ANTI_ALIAS, col::TEXT_ALIGNMENT],
        read: |s, w| {
            if let Some(aa) = int(s.variant, k::ANTI_ALIAS).and_then(|n| name_of(&TYPE_AA, n)) {
                w.options.type_aa = aa.to_owned();
            }
            if let Some(align) = int(s.variant, col::TEXT_ALIGNMENT).and_then(|n| name_of(&ALIGN, n)) {
                w.options.type_align = align.to_owned();
            }
        },
        write: |w, v, _| {
            // A stored step that reads as this setting stays (crisp and sharp share a step).
            if int(v, k::ANTI_ALIAS).and_then(|n| name_of(&TYPE_AA, n)) != Some(w.options.type_aa.as_str())
                && let Some(n) = number_of(&TYPE_AA, &w.options.type_aa)
            {
                put_int(v, k::ANTI_ALIAS, n);
            }
            if let Some(n) = number_of(&ALIGN, &w.options.type_align) {
                put_int(v, col::TEXT_ALIGNMENT, n);
            }
        },
        key: |w| json!([w.options.type_aa, w.options.type_align]),
    },
    Rule {
        id: "eyedropperSample",
        scope: Scope::Eyedropper,
        columns: &[col::PICKUP_TARGET],
        read: |s, w| {
            if let Some(n) = int(s.variant, col::PICKUP_TARGET) {
                w.options.eyedropper_sample = if n == 0 { "current" } else { "all" }.to_owned();
            }
        },
        write: |w, v, _| {
            let layer = w.options.eyedropper_sample == "current";
            if (int(v, col::PICKUP_TARGET) == Some(0)) != layer {
                put_int(v, col::PICKUP_TARGET, if layer { 0 } else { 1 });
            }
        },
        key: |w| json!(w.options.eyedropper_sample == "current"),
    },
    Rule {
        id: "autoSelect",
        scope: Scope::Move,
        columns: &[col::AUTO_SELECT_LAYER],
        read: |s, w| {
            if let Some(on) = flag(s.variant, col::AUTO_SELECT_LAYER) {
                w.options.move_auto_select = on;
            }
        },
        write: |w, v, _| put_flag(v, col::AUTO_SELECT_LAYER, w.options.move_auto_select),
        key: |w| json!(w.options.move_auto_select),
    },
];
