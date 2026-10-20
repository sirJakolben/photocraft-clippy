//! The Tool palette (S8, S17): one icon per tool of the stock, in its order, and the main, sub and
//! transparent colour under them.
//!
//! Interaction (G10):
//! - Click a tool: its Sub Tool palette shows, with the sub tool it used last selected (else the
//!   first of the group it shows). The selected tool is highlighted.
//! - A tool's icon is the icon of the group it shows (S17); it changes with the group tab.
//! - Right-click a tool: New Tool…, Rename Tool…, Delete Tool (to the trash), Save Brushes As…
//!   (a copy of the tool's folder, I6). Right-click the empty area: New Tool….
//! - Drag a tool onto another to move it there; drop a sub tool or a group on a tool to move it
//!   into that tool (Ctrl copies; `super::dnd`).
//! - Colours: the main colour chip draws with the main colour (a click on it while the
//!   transparent colour is on switches back, else opens the Color Picker); the sub colour chip
//!   opens the Color Picker for the background colour; the transparent colour chip makes the brush
//!   and pencil erase (PhotoCraft's Clear mode) until the main colour is chosen again.
//! - Without a stock: a note and Import Brushes….

use photocraft_clippy_stock::EntryPath;
use photocraft_engine::clippy_cmds::STOCK_DELETE;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::Tokens;
use serde_json::json;

use super::dialog::{self, Purpose};
use super::dnd::{self, Target};
use super::{Drag, brushes};

/// Side of a tool button.
const BUTTON: f32 = 30.0;

/// Draw the Tool palette's body.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin::same(6)).show(ui, |ui| {
        let Some(stock) = super::stock(app) else {
            let note = super::stock_error(app).unwrap_or_else(|| tl!("Reading the sub tools…").to_string());
            empty(app, ui, note);
            return;
        };
        if stock.tools.is_empty() {
            empty(app, ui, tl!("No sub tools – add or import them.").to_string());
            return;
        }
        let shown = super::shown_tool(app);
        let width = ui.available_width();
        let per_row = ((width + 2.0) / (BUTTON + 2.0)).floor().max(1.0) as usize;
        egui::Grid::new("clippy-tool-grid").spacing(egui::vec2(2.0, 2.0)).show(ui, |ui| {
            for (i, tool) in stock.tools.iter().enumerate() {
                let group = super::shown_group(app, &tool.dir);
                let icon = super::tool_icon(tool, group.as_deref());
                let selected = shown.as_deref() == Some(tool.dir.as_str());
                let tip = match &tool.shortcut {
                    Some(k) => format!("{} ({k})", tool.name),
                    None => tool.name.clone(),
                };
                let resp = photocraft_ui_egui::icons::button(ui, icon, BUTTON, selected, &tip);
                let resp = resp.interact(egui::Sense::click_and_drag());
                if resp.clicked()
                    && let Err(e) = super::select_tool(app, &tool.dir)
                {
                    app.ui.status = e;
                }
                dnd::source(&resp, Drag::Tool(tool.dir.clone()));
                dnd::target(app, ui, &resp, Target::Tool { tool: tool.dir.clone(), index: i });
                let dir = tool.dir.clone();
                let name = tool.name.clone();
                resp.context_menu(|ui| tool_menu(app, ui, Some((&dir, &name))));
                if (i + 1) % per_row == 0 {
                    ui.end_row();
                }
            }
        });
        // The rest of the palette takes a right-click too (New Tool… where there is no tool).
        let rest = ui.allocate_response(egui::vec2(ui.available_width(), 8.0), egui::Sense::click());
        rest.context_menu(|ui| tool_menu(app, ui, None));
        ui.add_space(4.0);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, t.separator));
        ui.add_space(6.0);
        colors(app, ui);
    });
}

/// The empty palette: why, and Import Brushes….
fn empty(app: &mut PhotocraftApp, ui: &mut egui::Ui, note: String) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(note).color(t.text_faint).small());
    ui.add_space(4.0);
    if photocraft_ui_egui::widgets::secondary_button(ui, tl!("Import Brushes…"), 0.0).clicked()
        && let Err(e) = brushes::import_dialog(app, None)
    {
        app.ui.status = e;
    }
    let rest = ui.allocate_response(ui.available_size().max(egui::vec2(1.0, 1.0)), egui::Sense::click());
    rest.context_menu(|ui| tool_menu(app, ui, None));
}

/// The context menu of a tool (`None`: the palette's empty area).
fn tool_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Option<(&str, &str)>) {
    if ui.button(tl!("New Tool…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::NewTool, "");
        ui.close();
    }
    let Some((dir, name)) = tool else { return };
    let path = EntryPath::Tool(dir.to_owned());
    if ui.button(tl!("New Sub Tool Group…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::NewGroup { tool: dir.to_owned() }, "");
        ui.close();
    }
    if ui.button(tl!("Rename Tool…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::Rename { path: path.clone() }, name);
        ui.close();
    }
    if ui.button(tl!("Delete Tool")).clicked() {
        if let Err(e) = app.run(STOCK_DELETE, json!({"path": path.to_string()})) {
            app.ui.status = e;
        } else {
            app.ui.status = format!("{name}: {}", tl!("moved to the trash."));
        }
        ui.close();
    }
    ui.separator();
    if ui.button(tl!("Save Brushes As…")).clicked() {
        if let Err(e) = brushes::export_dialog(app, Some(path)) {
            app.ui.status = e;
        }
        ui.close();
    }
}

/// Main, sub and transparent colour (S8).
fn colors(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let clear = app.ui.tool_options.brush_clear || app.ui.tool_options.pencil_clear;
    let w = ((ui.available_width() - 8.0) / 3.0).clamp(20.0, 48.0);
    let size = egui::vec2(w, 18.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let chip = |ui: &mut egui::Ui, on: bool, tip: &str, fill: Option<egui::Color32>| {
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let p = ui.painter();
            match fill {
                Some(c) => {
                    p.rect_filled(rect, 2.0, c);
                }
                None => photocraft_ui_egui::widgets::checker(p, rect, 4.0),
            }
            let stroke = if on { egui::Stroke::new(2.0, t.accent) } else { egui::Stroke::new(1.0, t.field_border) };
            p.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Outside);
            let resp = resp.on_hover_text(tl!(tip));
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, tl!(tip)));
            resp
        };
        let fg = app.session.tools.foreground;
        let bg = app.session.tools.background;
        let c = |rgb: [f32; 4]| egui::Color32::from_rgb((rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8);
        if chip(ui, !clear, "Main color", Some(c(fg))).clicked() {
            if clear {
                set_clear(app, false);
            } else {
                photocraft_ui_egui::color_picker_ui::open(app, "foreground");
            }
        }
        if chip(ui, false, "Sub color", Some(c(bg))).clicked() {
            photocraft_ui_egui::color_picker_ui::open(app, "background");
        }
        if chip(ui, clear, "Transparent color", None).clicked() {
            set_clear(app, !clear);
        }
    });
}

/// The transparent colour: PhotoCraft's Clear mode of the brush and the pencil.
fn set_clear(app: &mut PhotocraftApp, on: bool) {
    app.ui.tool_options.brush_clear = on;
    app.ui.tool_options.pencil_clear = on;
}
