//! The Tool Property palette (S9–S12): the active sub tool's stroke preview with its name and the
//! lock (E10), its settings with the eye on (`VariantShowParam`, S10) as Clip Studio Paint's
//! controls (S11), and the Sub Tool Detail button (S12; the window comes with M11).
//!
//! Interaction (G10):
//! - A setting edits PhotoCraft's working copy at once; the data flow writes it into the `.sut`
//!   once the pointer is up (`crate::subtool`).
//! - The lock: a locked sub tool keeps its file as it is; changes last until it is selected again.
//! - When PhotoCraft's tool is another one now (a shortcut, PhotoCraft's tool column), the settings
//!   give way to a note and a button that selects the sub tool again, so no setting lands in
//!   another tool's brush.
//! - A sub tool PhotoCraft has no tool for shows why; one without settings to show says so.

use photocraft_engine::clippy_cmds::SUBTOOL_LOCK;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::Tokens;
use serde_json::json;

use super::{preview, quick};

/// Height of the preview strip.
const HEADER_HEIGHT: f32 = 34.0;

/// Draw the Tool Property palette's body.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(info) = crate::subtool::info(app) else {
        egui::Frame::NONE.inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("No sub tool is selected.")).color(t.text_faint).small());
        });
        return;
    };
    let footer = photocraft_ui_egui::widgets::footer_height(ui);
    let body = (ui.available_height() - footer).max(0.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), body), |ui| {
        header(app, ui, &info);
        egui::ScrollArea::vertical().id_salt("clippy-tool-property").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| settings(app, ui, &info));
        });
    });
    photocraft_ui_egui::widgets::panel_footer(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Sub Tool Detail (E1) is M11's.
            ui.add_enabled_ui(false, |ui| photocraft_ui_egui::icons::button(ui, "settings", 22.0, false, tl!("Sub Tool Detail (in a later version)")));
        });
    });
}

/// The preview strip: the stroke, the name at its top left, the lock at its right.
fn header(app: &mut PhotocraftApp, ui: &mut egui::Ui, info: &crate::subtool::Info) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), HEADER_HEIGHT), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.field);
    if let Some(brush) = crate::subtool::live_brush(app).or_else(|| file_brush(app, ui)) {
        let stroke = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 2.0), rect.max - egui::vec2(26.0, 2.0));
        preview::paint(ui, stroke, "clippy-tool-property", &brush, t.text);
    }
    let galley = ui.painter().layout_no_wrap(info.name.clone(), egui::FontId::proportional(11.0), t.text);
    let label = egui::Rect::from_min_size(rect.min, galley.size() + egui::vec2(8.0, 2.0));
    ui.painter().rect_filled(label, 0.0, t.card);
    ui.painter().galley(label.min + egui::vec2(4.0, 1.0), galley, t.text);
    let lock = egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0, rect.bottom() - 10.0), egui::vec2(18.0, 18.0));
    let resp = ui.interact(lock, ui.id().with("clippy-lock"), egui::Sense::click());
    let tip = if info.locked { tl!("Unlock the sub tool") } else { tl!("Lock the sub tool: changes are not saved") };
    photocraft_ui_egui::icons::paint(
        ui,
        lock,
        if info.locked { "lock" } else { "lock-open" },
        14.0,
        if resp.hovered() || info.locked { t.text } else { t.text_faint },
    );
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, info.locked, tl!("Lock")));
    if resp.clicked()
        && let Err(e) = app.run(SUBTOOL_LOCK, json!({"on": !info.locked}))
    {
        app.ui.status = e;
    }
}

/// The active sub tool's brush as its file holds it (when PhotoCraft's brush is another tool's).
fn file_brush(app: &mut PhotocraftApp, ui: &egui::Ui) -> Option<photocraft_engine::BrushSettings> {
    let path = app.session.prefs().clippy.subtool.clone();
    let frame = ui.ctx().cumulative_pass_nr();
    super::view(app).previews.brush(std::path::Path::new(&path), frame).ok().flatten().map(|b| (*b).clone())
}

/// The settings with the eye on, or why there are none.
fn settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, info: &crate::subtool::Info) {
    let t = Tokens::get(ui.ctx());
    let faint = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).color(t.text_faint).small());
    };
    let Some(tool) = info.tool else {
        faint(ui, &format!("{}. {}", info.notice.clone().unwrap_or_default(), tl!("This sub tool has no settings PhotoCraft can use.")));
        return;
    };
    if !info.current {
        faint(ui, tl!("Another tool is in use. Select the sub tool again to change its settings."));
        if photocraft_ui_egui::widgets::secondary_button(ui, tl!("Select Sub Tool"), 0.0).clicked()
            && let Err(e) = crate::subtool::reselect(app)
        {
            app.ui.status = e;
        }
        return;
    }
    let shown = quick::settings(&info.show, tool);
    if shown.is_empty() {
        faint(ui, tl!("No settings"));
        return;
    }
    for q in shown {
        quick::show(app, ui, q, tool);
        ui.add_space(2.0);
    }
}
