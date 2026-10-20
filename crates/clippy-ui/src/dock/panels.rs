//! Every palette Clippy's docks can hold (`docs/paletten.md` in the planning notes): its id in
//! the saved layout, title, icon, size and what draws its body.

use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::dock::Group;
use photocraft_ui_egui::theme::Tokens;
use serde::{Deserialize, Serialize};

/// A palette. The camelCase name is its id in the saved layout, so variants are never renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanelId {
    /// The document area; protected (never moved, floated or closed).
    Canvas,
    Tool,
    SubTool,
    ToolProperty,
    CommandBar,
    ColorWheel,
    ColorSlider,
    ColorSet,
    IntermediateColor,
    ColorHistory,
    ColorMixing,
    Gradients,
    Patterns,
    Navigator,
    Info,
    Histogram,
    Layer,
    Channels,
    Paths,
    History,
    AutoAction,
    LayerComps,
    LayerProperty,
    Adjustments,
    Character,
    Paragraph,
}

/// What draws a palette's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Canvas,
    /// PhotoCraft's dock panel of that group and tab label.
    PhotoCraft(Group, &'static str),
    /// Clippy's Tool palette (M9).
    Tools,
    /// Clippy's Sub Tool palette (M9).
    SubTools,
    /// Clippy's Tool Property palette (M9).
    ToolProperty,
    /// A Clippy palette that isn't built yet: a note in its place.
    Planned,
}

/// A palette's registration.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    pub id: PanelId,
    /// English title, the key of its translation (PhotoCraft's own for PhotoCraft's panels).
    pub title: &'static str,
    /// A PhotoCraft icon name (`photocraft_ui_egui::icons`); the icon-only tabs of M3 use it.
    pub icon: &'static str,
    /// Smallest body height, points.
    pub min_height: f32,
    pub body: Body,
}

const fn panel(id: PanelId, title: &'static str, icon: &'static str, min_height: f32, body: Body) -> Panel {
    Panel { id, title, icon, min_height, body }
}

/// Every palette, in the order Window menus list them.
pub const PANELS: [Panel; 26] = [
    panel(PanelId::Canvas, "Canvas", "image", 0.0, Body::Canvas),
    panel(PanelId::Tool, "Tool", "brush", 120.0, Body::Tools),
    panel(PanelId::SubTool, "Sub Tool", "panels-top-left", 80.0, Body::SubTools),
    panel(PanelId::ToolProperty, "Tool Property", "sliders-horizontal", 80.0, Body::ToolProperty),
    panel(PanelId::CommandBar, "Command Bar", "app-window", 32.0, Body::Planned),
    panel(PanelId::ColorWheel, "Color Wheel", "palette", 80.0, Body::Planned),
    panel(PanelId::ColorSlider, "Color", "contrast", 80.0, Body::PhotoCraft(Group::Color, "Color")),
    panel(PanelId::ColorSet, "Swatches", "swatch-book", 80.0, Body::PhotoCraft(Group::Color, "Swatches")),
    panel(PanelId::IntermediateColor, "Intermediate Color", "blend", 80.0, Body::Planned),
    panel(PanelId::ColorHistory, "Color History", "clock", 80.0, Body::Planned),
    panel(PanelId::ColorMixing, "Color Mixing", "droplet", 80.0, Body::Planned),
    panel(PanelId::Gradients, "Gradients", "gradient", 80.0, Body::PhotoCraft(Group::Color, "Gradients")),
    panel(PanelId::Patterns, "Patterns", "grid-3x3", 80.0, Body::PhotoCraft(Group::Color, "Patterns")),
    panel(PanelId::Navigator, "Navigator", "compass", 80.0, Body::PhotoCraft(Group::Navigator, "Navigator")),
    panel(PanelId::Info, "Info", "info", 80.0, Body::PhotoCraft(Group::Navigator, "Info")),
    panel(PanelId::Histogram, "Histogram", "gauge", 80.0, Body::PhotoCraft(Group::Navigator, "Histogram")),
    // Layers lays out its own row list and footer: room for both (as in PhotoCraft's dock).
    panel(PanelId::Layer, "Layers", "layers", 220.0, Body::PhotoCraft(Group::Layers, "Layers")),
    panel(PanelId::Channels, "Channels", "layer-mask", 80.0, Body::PhotoCraft(Group::Layers, "Channels")),
    panel(PanelId::Paths, "Paths", "pen-tool", 80.0, Body::PhotoCraft(Group::Layers, "Paths")),
    panel(PanelId::History, "History", "undo-2", 80.0, Body::PhotoCraft(Group::History, "History")),
    panel(PanelId::AutoAction, "Actions", "play", 80.0, Body::PhotoCraft(Group::History, "Actions")),
    panel(PanelId::LayerComps, "Layer Comps", "copy", 80.0, Body::PhotoCraft(Group::History, "Layer Comps")),
    panel(PanelId::LayerProperty, "Properties", "settings", 80.0, Body::PhotoCraft(Group::Properties, "Properties")),
    panel(PanelId::Adjustments, "Adjustments", "adjustment-layer", 80.0, Body::PhotoCraft(Group::Properties, "Adjustments")),
    panel(PanelId::Character, "Character", "type", 80.0, Body::PhotoCraft(Group::Character, "Character")),
    panel(PanelId::Paragraph, "Paragraph", "align-left", 80.0, Body::PhotoCraft(Group::Character, "Paragraph")),
];

/// Narrowest a palette gets (`docs/plan/phase1.md` M3: 160 px).
pub const MIN_WIDTH: f32 = 160.0;

impl PanelId {
    pub fn info(self) -> &'static Panel {
        // PANELS lists every variant once, in declaration order (a test checks it).
        const FALLBACK: Panel = PANELS[0];
        PANELS.get(self as usize).unwrap_or(&FALLBACK)
    }

    /// The translated title.
    pub fn title(self) -> String {
        tl!(self.info().title).to_string()
    }
}

/// Draw `id`'s body into `ui`, the palette's whole content rect.
pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: PanelId) {
    match id.info().body {
        Body::Canvas => canvas(app, ui),
        Body::PhotoCraft(group, label) => {
            egui::Frame::NONE.inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                photocraft_ui_egui::extension::photocraft_panel(app, ui, group, label);
            });
        }
        Body::Tools => crate::palettes::tools::show(app, ui),
        Body::SubTools => crate::palettes::subtools::show(app, ui),
        Body::ToolProperty => crate::palettes::property::show(app, ui),
        Body::Planned => planned(ui, id),
    }
}

/// The document area on PhotoCraft's backdrop, as its central panel draws it.
fn canvas(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let backdrop = photocraft_ui_egui::prefs_ui::pasteboard_color(app).unwrap_or(t.canvas);
    ui.painter().rect_filled(ui.max_rect(), 0.0, backdrop);
    photocraft_ui_egui::canvas::document_area(app, ui);
}

/// A palette still to come: its name and a note, so the default layout already has its place.
fn planned(ui: &mut egui::Ui, id: PanelId) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.label(egui::RichText::new(id.title()).color(t.text_dim));
        ui.label(egui::RichText::new(tl!("This palette comes in a later version of photocraft-clippy.")).color(t.text_faint).small());
    });
}
