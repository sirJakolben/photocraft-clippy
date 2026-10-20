//! The Tool Property palette's settings (S10, S11): a sub tool's `VariantShowParam` lists the ids
//! of the settings whose eye is on, in the order the palette shows them. This table gives the ids
//! PhotoCraft has a counterpart for (U3) their name, control and the field of PhotoCraft's working
//! copy they edit; ids it doesn't list (vector magnet, vector eraser, texture, spray particles …)
//! are not shown.
//!
//! The ids were matched to the settings by comparing Clip Studio Paint 1.5's Tool Property palettes
//! (the screenshots behind `docs/subtool-standard.md` in the planning notes) with the lists in the
//! same tool database: the G-Pen's `1000, 10, 20, 40, 41, 42, 1241` are Brush Size, Opacity,
//! Anti-aliasing, Stabilization, Adjust by speed (with an unnamed sub-item) and Vector magnet; the
//! soft airbrush, which hides Opacity, lacks 10. A setting edits the working copy; the data flow
//! (`crate::subtool`) writes it into the `.sut` once the pointer is up.

use photocraft_color::BlendMode;
use photocraft_engine::BrushSettings;
use photocraft_engine::paint::brush::SpeedSmoothing;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::state::Tool;
use photocraft_ui_egui::widgets;

use super::controls::{self, Scale};
use crate::mapping::Scope;

/// Indent of a sub-item under its setting.
const SUB_INDENT: f32 = 18.0;

/// What a setting edits and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Size,
    Opacity,
    Blend,
    AntiAlias,
    Hardness,
    Density,
    Stabilization,
    BySpeed,
    ContinuousSpray,
    Thickness,
    WaterEdge,
    PaintAmount,
    PaintDensity,
    ColorStretch,
    BlurStrength,
    SelectionMode,
    Contiguous,
    ColorMargin,
    MultipleReferencing,
    FillAntiAlias,
    Sample,
    AutoSelect,
    FontSize,
}

/// One setting of the table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quick {
    pub id: u32,
    /// Clip Studio Paint's English name: the translation key.
    pub label: &'static str,
    pub kind: Kind,
}

const fn q(id: u32, label: &'static str, kind: Kind) -> Quick {
    Quick { id, label, kind }
}

/// Every setting the palette can show, by `VariantShowParam` id.
pub(crate) const QUICK: &[Quick] = &[
    q(1000, "Brush Size", Kind::Size),
    q(10, "Opacity", Kind::Opacity),
    q(30, "Blending mode", Kind::Blend),
    q(20, "Anti-aliasing", Kind::AntiAlias),
    q(1030, "Hardness", Kind::Hardness),
    q(1020, "Brush density", Kind::Density),
    q(40, "Stabilization", Kind::Stabilization),
    q(41, "Adjust by speed", Kind::BySpeed),
    q(1043, "Continuous spray", Kind::ContinuousSpray),
    q(1050, "Thickness", Kind::Thickness),
    q(1201, "Watercolor edge", Kind::WaterEdge),
    q(1081, "Amount of paint", Kind::PaintAmount),
    q(1083, "Density of paint", Kind::PaintDensity),
    q(1085, "Color stretch", Kind::ColorStretch),
    q(1091, "Blur strength", Kind::BlurStrength),
    q(3000, "Selection mode", Kind::SelectionMode),
    q(5200, "Follow adjacent pixels", Kind::Contiguous),
    q(5120, "Color margin", Kind::ColorMargin),
    q(5001, "Multiple referencing", Kind::MultipleReferencing),
    q(5130, "Anti-aliasing", Kind::FillAntiAlias),
    q(4100, "Sample", Kind::Sample),
    q(4010, "Move the object at the clicked position", Kind::AutoSelect),
    q(7001, "Size", Kind::FontSize),
];

impl Kind {
    /// Whether the setting applies to a sub tool mapped to `tool`.
    fn applies(self, tool: Tool) -> bool {
        use Kind::*;
        match self {
            Size | Blend | Hardness | Density | Stabilization | BySpeed | ContinuousSpray | Thickness | WaterEdge => Scope::Brush.applies(tool),
            Opacity => Scope::Brush.applies(tool) || Scope::Fill.applies(tool),
            AntiAlias => Scope::Brush.applies(tool) || Scope::Selection.applies(tool),
            PaintAmount | PaintDensity | ColorStretch => Scope::Mixer.applies(tool),
            BlurStrength => Scope::Blur.applies(tool),
            SelectionMode => Scope::Selection.applies(tool),
            Contiguous | ColorMargin | MultipleReferencing | FillAntiAlias => Scope::Region.applies(tool),
            Sample => Scope::Eyedropper.applies(tool),
            AutoSelect => Scope::Move.applies(tool),
            FontSize => Scope::Text.applies(tool),
        }
    }
}

/// The settings to show for the ids `ids` of a sub tool mapped to `tool`, in their order.
pub(crate) fn settings(ids: &[u32], tool: Tool) -> Vec<&'static Quick> {
    let mut out: Vec<&'static Quick> = Vec::new();
    for id in ids {
        if let Some(q) = QUICK.iter().find(|q| q.id == *id && q.kind.applies(tool))
            && !out.iter().any(|o| o.id == q.id)
        {
            out.push(q);
        }
    }
    out
}

/// Change the brush through `f`; a change counts as one (`brush_changed`), so the data flow
/// writes it back.
fn edit_brush(app: &mut PhotocraftApp, f: impl FnOnce(&mut BrushSettings)) {
    let mut b = app.session.tools.brush.clone();
    f(&mut b);
    if b != app.session.tools.brush {
        app.session.tools.brush = b;
        app.session.tools.brush_changed();
    }
}

/// Draw the setting `q` for a sub tool mapped to `tool`.
pub(crate) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, q: &Quick, tool: Tool) {
    let label = q.label;
    let b = app.session.tools.brush.clone();
    match q.kind {
        Kind::Size => {
            let mut v = b.size;
            if controls::value_row(ui, label, &mut v, 0.5..=2000.0, 1, Scale::Sqrt, 0.0) {
                edit_brush(app, |b| b.size = v);
            }
        }
        Kind::Opacity if Scope::Fill.applies(tool) => {
            let mut v = app.ui.tool_options.fill_opacity;
            if controls::value_row(ui, label, &mut v, 0.0..=100.0, 0, Scale::Linear, 0.0) {
                app.ui.tool_options.fill_opacity = v;
            }
        }
        Kind::Opacity => percent(app, ui, label, b.opacity, |b, v| b.opacity = v),
        Kind::Blend => {
            let mut mode = b.mode;
            let options: Vec<(BlendMode, &str)> = BlendMode::LAYER_MODES.iter().map(|m| (*m, m.label())).collect();
            ui.horizontal(|ui| {
                let t = photocraft_ui_egui::theme::Tokens::get(ui.ctx());
                ui.label(egui::RichText::new(tl!(label)).color(t.text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::dropdown(ui, "clippy-quick-blend", &mut mode, &options, 120.0) {
                        edit_brush(app, |b| b.mode = mode);
                    }
                });
            });
        }
        Kind::AntiAlias if Scope::Selection.applies(tool) => {
            let mut on = app.ui.tool_options.anti_alias;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                app.ui.tool_options.anti_alias = on;
            }
        }
        Kind::AntiAlias => {
            let mut level = b.aa_level.map_or(if b.aliased { 0 } else { 2 }, |l| l.min(3));
            if controls::steps(ui, label, &mut level, 4, &["None", "Weak", "Medium", "Strong"]) {
                edit_brush(app, |b| {
                    b.aa_level = Some(level);
                    b.aliased = level == 0;
                });
            }
        }
        Kind::Hardness => {
            let mut v = b.hardness;
            if controls::cells(ui, label, &mut v) {
                edit_brush(app, |b| b.hardness = v);
            }
        }
        Kind::Density => percent(app, ui, label, b.flow, |b, v| b.flow = v),
        Kind::Stabilization => percent(app, ui, label, b.smoothing.amount, |b, v| b.smoothing.amount = v),
        Kind::BySpeed => {
            let mut on = b.smoothing.by_speed.is_some();
            if controls::check(ui, label, &mut on, SUB_INDENT).changed() {
                edit_brush(app, |b| b.smoothing.by_speed = on.then_some(b.smoothing.by_speed.unwrap_or(SpeedSmoothing::StrengthenSlow)));
            }
        }
        Kind::ContinuousSpray => {
            let mut on = b.build_up;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                edit_brush(app, |b| b.build_up = on);
            }
        }
        Kind::Thickness => percent(app, ui, label, b.roundness, |b, v| b.roundness = v.max(0.01)),
        Kind::WaterEdge => {
            let mut on = b.wet_edges;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                edit_brush(app, |b| b.wet_edges = on);
            }
        }
        // Clip Studio Paint's amounts of paint are the share of the brush colour; the mixer's
        // `mix` and `wet` are the canvas's share (the mapping's reading).
        Kind::PaintAmount => percent(app, ui, label, 1.0 - b.mixer.mix, |b, v| b.mixer.mix = 1.0 - v),
        Kind::PaintDensity => percent(app, ui, label, 1.0 - b.mixer.wet, |b, v| b.mixer.wet = 1.0 - v),
        Kind::ColorStretch => percent(app, ui, label, b.mixer.load, |b, v| b.mixer.load = v),
        Kind::BlurStrength => {
            let mut v = app.ui.tool_options.strength;
            if controls::value_row(ui, label, &mut v, 0.0..=100.0, 0, Scale::Linear, 0.0) {
                app.ui.tool_options.strength = v;
            }
        }
        Kind::SelectionMode => {
            let mut mode = app.ui.selection_mode.min(3);
            if controls::steps_icons(
                ui,
                label,
                &mut mode,
                &[
                    ("square", "New selection"),
                    ("plus", "Add to selection"),
                    ("minus", "Subtract from selection"),
                    ("squares-subtract", "Intersect with selection"),
                ],
            ) {
                app.ui.selection_mode = mode;
            }
        }
        Kind::Contiguous => {
            let mut on = app.ui.tool_options.contiguous;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                app.ui.tool_options.contiguous = on;
            }
        }
        Kind::ColorMargin => {
            // PhotoCraft's tolerance is 0–255; Clip Studio Paint's color margin a percentage.
            let mut v = app.ui.tool_options.tolerance / 255.0 * 100.0;
            if controls::value_row(ui, label, &mut v, 0.0..=100.0, 1, Scale::Linear, 0.0) {
                app.ui.tool_options.tolerance = v / 100.0 * 255.0;
            }
        }
        Kind::MultipleReferencing => {
            let mut on = app.ui.tool_options.sample_all_layers;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                app.ui.tool_options.sample_all_layers = on;
            }
        }
        Kind::FillAntiAlias => {
            let mut on = app.ui.tool_options.anti_alias;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                app.ui.tool_options.anti_alias = on;
            }
        }
        Kind::Sample => {
            let mut sample = if app.ui.tool_options.eyedropper_sample == "current" { "current" } else { "all" };
            ui.horizontal(|ui| {
                let t = photocraft_ui_egui::theme::Tokens::get(ui.ctx());
                ui.label(egui::RichText::new(tl!(label)).color(t.text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::dropdown(ui, "clippy-quick-sample", &mut sample, &[("current", "Current Layer"), ("all", "All Layers")], 120.0) {
                        app.ui.tool_options.eyedropper_sample = sample.to_owned();
                    }
                });
            });
        }
        Kind::AutoSelect => {
            let mut on = app.ui.tool_options.move_auto_select;
            if controls::check(ui, label, &mut on, 0.0).changed() {
                app.ui.tool_options.move_auto_select = on;
            }
        }
        Kind::FontSize => {
            let mut v = app.ui.tool_options.type_size;
            if controls::value_row(ui, label, &mut v, 1.0..=1296.0, 1, Scale::Sqrt, 0.0) {
                app.ui.tool_options.type_size = v;
            }
        }
    }
}

/// A 0–1 brush value shown as 0–100.
fn percent(app: &mut PhotocraftApp, ui: &mut egui::Ui, label: &str, value: f32, set: impl FnOnce(&mut BrushSettings, f32)) {
    let mut v = (value * 100.0).clamp(0.0, 100.0);
    if controls::value_row(ui, label, &mut v, 0.0..=100.0, 0, Scale::Linear, 0.0) {
        edit_brush(app, |b| set(b, v / 100.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_listed_once_per_kind_of_tool() {
        for (i, a) in QUICK.iter().enumerate() {
            for b in QUICK.iter().skip(i + 1) {
                // One entry per id; the tool decides how it shows (anti-aliasing is steps on a
                // brush, a check box on a selection).
                assert!(a.id != b.id, "{} listed twice", a.id);
            }
        }
    }

    #[test]
    fn the_g_pen_shows_its_five_mapped_settings_in_file_order() {
        let ids = [1000, 10, 20, 40, 41, 42, 1241];
        let shown: Vec<u32> = settings(&ids, Tool::Brush).iter().map(|q| q.id).collect();
        assert_eq!(shown, [1000, 10, 20, 40, 41]);
    }

    #[test]
    fn settings_of_other_tools_are_left_out() {
        // A fill's settings on a brush: nothing; a brush size on a selection: nothing.
        assert!(settings(&[5200, 5120], Tool::Brush).is_empty());
        assert!(settings(&[1000], Tool::RectMarquee).is_empty());
        let fill: Vec<u32> = settings(&[5200, 5101, 5120, 5111, 5112, 5001, 10, 5130], Tool::PaintBucket).iter().map(|q| q.id).collect();
        assert_eq!(fill, [5200, 5120, 5001, 10, 5130]);
    }
}
