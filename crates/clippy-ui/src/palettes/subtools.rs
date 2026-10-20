//! The Sub Tool palette (S2–S7, S15, S16): the groups of the tool shown as tabs (icon and name),
//! under them the group's sub tools as tiles in two columns (stroke preview and name, no icon),
//! "Add Sub Tool" and the New, Duplicate and Delete buttons.
//!
//! Interaction (G10):
//! - Click a tab: its group shows and the sub tool it used last is selected (else its first).
//! - Click a tile: the sub tool is selected (`clippy.subtool.select`) and highlighted. A tile with
//!   a warning (a broken `.sut`, P10) shows the warning as tooltip and can't be selected.
//! - Drag a tile onto another tile, a tab or a tool icon; drag a tab onto another tab or a tool
//!   icon (Ctrl copies; `super::dnd`).
//! - Right-click a tab: New Sub Tool Group…, Rename…, Icon (S15), Delete, Save Brushes As… (I6).
//!   Right-click a tile: New Sub Tool…, Duplicate, Rename…, Delete, Save Brushes As…. Deleting
//!   moves to the trash.
//! - Add Sub Tool: import `.sut` files into the group shown (I3). New: a sub tool from PhotoCraft's
//!   current tool and brush; Duplicate and Delete act on the selected sub tool.
//! - Empty states: no stock, a tool without groups, a group without sub tools each say what to do.

use photocraft_clippy_stock::{EntryPath, GroupEntry, Stock};
use photocraft_engine::clippy_cmds::{STOCK_COPY, STOCK_DELETE, STOCK_META_SET};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::Tokens;
use serde_json::{Value, json};

use super::dialog::{self, Purpose};
use super::dnd::{self, Target};
use super::{Drag, brushes, preview};

/// Height of a tile.
const TILE_HEIGHT: f32 = 30.0;
/// Height of a group tab.
const TAB_HEIGHT: f32 = 22.0;

/// Draw the Sub Tool palette's body.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(stock) = super::stock(app) else {
        note(ui, &super::stock_error(app).unwrap_or_else(|| tl!("Reading the sub tools…").to_string()));
        return;
    };
    let Some(tool) = super::shown_tool(app) else {
        note(ui, tl!("No sub tools – add or import them."));
        if photocraft_ui_egui::widgets::secondary_button(ui, tl!("Import Brushes…"), 0.0).clicked()
            && let Err(e) = brushes::import_dialog(app, None)
        {
            app.ui.status = e;
        }
        return;
    };
    let Some(entry) = stock.tool(&tool) else { return };
    let group = super::shown_group(app, &tool);
    let footer = photocraft_ui_egui::widgets::footer_height(ui);
    let body = ui.available_height() - footer;
    ui.allocate_ui(egui::vec2(ui.available_width(), body.max(0.0)), |ui| {
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
            tabs(app, ui, &stock, &tool, group.as_deref());
            ui.add_space(4.0);
            match group.as_deref().and_then(|g| entry.groups.iter().find(|x| x.dir == g)) {
                None => {
                    note(ui, tl!("This tool has no sub tool groups – right-click to add one."));
                    let rest = ui.allocate_response(ui.available_size().max(egui::vec2(1.0, 1.0)), egui::Sense::click());
                    rest.context_menu(|ui| group_menu(app, ui, &tool, None));
                }
                Some(g) => tiles(app, ui, &tool, g),
            }
        });
    });
    footer_buttons(app, ui, &tool, group.as_deref());
}

fn note(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.label(egui::RichText::new(text).color(t.text_faint).small());
    });
}

/// The group tabs, two per row.
fn tabs(app: &mut PhotocraftApp, ui: &mut egui::Ui, stock: &Stock, tool: &str, shown: Option<&str>) {
    let Some(entry) = stock.tool(tool) else { return };
    let t = Tokens::get(ui.ctx());
    let w = ((ui.available_width() - 2.0) / 2.0).max(40.0);
    for (row, pair) in entry.groups.chunks(2).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (col, g) in pair.iter().enumerate() {
                let index = row * 2 + col;
                let on = shown == Some(g.dir.as_str());
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, TAB_HEIGHT), egui::Sense::click_and_drag());
                let p = ui.painter();
                p.rect_filled(
                    rect,
                    3.0,
                    if on {
                        t.accent_soft
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        t.field
                    },
                );
                p.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, if on { t.accent_border } else { t.field_border }), egui::StrokeKind::Inside);
                let icon = egui::Rect::from_min_size(rect.min + egui::vec2(5.0, 4.0), egui::vec2(14.0, 14.0));
                photocraft_ui_egui::icons::paint(ui, icon, super::group_icon(g), 14.0, if on { t.text } else { t.text_dim });
                let text_rect = egui::Rect::from_min_max(egui::pos2(icon.right() + 5.0, rect.top()), rect.max - egui::vec2(4.0, 0.0));
                let galley = ui.painter().layout(g.name.clone(), egui::FontId::proportional(12.0), if on { t.text } else { t.text_dim }, text_rect.width());
                ui.painter().with_clip_rect(text_rect).galley(egui::pos2(text_rect.left(), rect.center().y - galley.size().y / 2.0), galley, t.text);
                let resp = resp.on_hover_text(&g.name);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, &g.name));
                if resp.clicked()
                    && let Err(e) = super::select_group(app, tool, &g.dir)
                {
                    app.ui.status = e;
                }
                let path = EntryPath::Group(tool.to_owned(), g.dir.clone());
                dnd::source(&resp, Drag::Group(path.clone()));
                dnd::target(app, ui, &resp, Target::Tab { group: path, index });
                resp.context_menu(|ui| group_menu(app, ui, tool, Some(g)));
            }
        });
    }
}

/// The tiles of group `g`, two per row, then Add Sub Tool.
fn tiles(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: &str, g: &GroupEntry) {
    let t = Tokens::get(ui.ctx());
    let active = super::active_entry(app);
    let root = super::root(app);
    let live = crate::subtool::live_brush(app);
    let frame = ui.ctx().cumulative_pass_nr();
    let group_path = EntryPath::Group(tool.to_owned(), g.dir.clone());
    let w = ((ui.available_width() - 2.0) / 2.0).max(40.0);
    let mut pending = false;
    egui::ScrollArea::vertical().id_salt(("clippy-subtools", tool, &g.dir)).auto_shrink([false, false]).show(ui, |ui| {
        for (row, pair) in g.subtools.chunks(2).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (col, s) in pair.iter().enumerate() {
                    let index = row * 2 + col;
                    let path = super::subtool_path(tool, &g.dir, s);
                    let on = active.as_ref() == Some(&path);
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, TILE_HEIGHT), egui::Sense::click_and_drag());
                    let p = ui.painter();
                    p.rect_filled(
                        rect,
                        2.0,
                        if on {
                            t.accent
                        } else if resp.hovered() {
                            t.hover
                        } else {
                            t.field
                        },
                    );
                    p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
                    let ink = if on { t.primary_text } else { t.text };
                    if s.warning.is_some() {
                        let icon = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 8.0), egui::vec2(14.0, 14.0));
                        photocraft_ui_egui::icons::paint(ui, icon, "triangle-alert", 14.0, t.warning);
                    } else {
                        let stroke = egui::Rect::from_min_max(rect.min + egui::vec2(2.0, 1.0), rect.max - egui::vec2(2.0, 11.0));
                        let brush = if on { live.clone().map(std::sync::Arc::new) } else { None };
                        let brush = match (brush, &root) {
                            (Some(b), _) => Some(b),
                            (None, Some(root)) => match super::view(app).previews.brush(&path.to_path(root), frame) {
                                Ok(b) => b,
                                Err(()) => {
                                    pending = true;
                                    None
                                }
                            },
                            (None, None) => None,
                        };
                        if let Some(b) = brush {
                            preview::paint(ui, stroke, &format!("clippy-tile:{path}"), &b, ink);
                        }
                    }
                    let galley = ui.painter().layout_no_wrap(s.name.clone(), egui::FontId::proportional(11.0), ink);
                    let name_pos = egui::pos2((rect.right() - 4.0 - galley.size().x).max(rect.left() + 20.0), rect.bottom() - galley.size().y - 1.0);
                    ui.painter().with_clip_rect(rect.shrink(1.0)).galley(name_pos, galley, ink);
                    let tip = match &s.warning {
                        Some(w) => format!("{}\n{w}", s.name),
                        None => s.name.clone(),
                    };
                    let resp = resp.on_hover_text(tip);
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, s.warning.is_none(), on, &s.name));
                    if resp.clicked()
                        && s.warning.is_none()
                        && !on
                        && let Err(e) = super::select(app, &path)
                    {
                        app.ui.status = e;
                    }
                    dnd::source(&resp, Drag::SubTool(path.clone()));
                    dnd::target(app, ui, &resp, Target::Tile { group: group_path.clone(), index });
                    let name = s.name.clone();
                    resp.context_menu(|ui| subtool_menu(app, ui, &group_path, Some((&path, &name))));
                }
            });
        }
        if g.subtools.is_empty() {
            note(ui, tl!("No sub tools in this group – add or import them."));
        }
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            let add = ui.add(egui::Button::new(egui::RichText::new(format!("⊕ {}", tl!("Add Sub Tool"))).color(t.text_dim)).frame(false));
            if add.clicked()
                && let Err(e) = brushes::import_dialog(app, Some(group_path.clone()))
            {
                app.ui.status = e;
            }
        });
        // The empty space under the tiles: a drop there moves into the group, last.
        let rest = ui.allocate_response(ui.available_size().max(egui::vec2(1.0, 24.0)), egui::Sense::click());
        dnd::target(app, ui, &rest, Target::Tile { group: group_path.clone(), index: g.subtools.len() });
        rest.context_menu(|ui| subtool_menu(app, ui, &group_path, None));
    });
    if pending {
        ui.ctx().request_repaint();
    }
    super::view(app).previews.trim(1024);
}

/// The context menu of a group tab (`None`: the empty palette of a tool without groups).
fn group_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: &str, g: Option<&GroupEntry>) {
    if ui.button(tl!("New Sub Tool Group…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::NewGroup { tool: tool.to_owned() }, "");
        ui.close();
    }
    let Some(g) = g else { return };
    let path = EntryPath::Group(tool.to_owned(), g.dir.clone());
    if ui.button(tl!("Rename Sub Tool Group…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::Rename { path: path.clone() }, &g.name);
        ui.close();
    }
    ui.menu_button(tl!("Icon"), |ui| {
        let t = Tokens::get(ui.ctx());
        egui::Grid::new("clippy-group-icons").spacing(egui::vec2(2.0, 2.0)).show(ui, |ui| {
            for (i, icon) in super::GROUP_ICONS.iter().enumerate() {
                let on = g.icon.as_deref() == Some(*icon);
                if photocraft_ui_egui::icons::button(ui, icon, 24.0, on, icon).clicked() {
                    meta_icon(app, &path, json!(icon));
                    ui.close();
                }
                if (i + 1) % 7 == 0 {
                    ui.end_row();
                }
            }
        });
        ui.separator();
        if ui.button(egui::RichText::new(tl!("Default Icon")).color(t.text)).clicked() {
            meta_icon(app, &path, Value::Null);
            ui.close();
        }
    });
    if ui.button(tl!("Delete Sub Tool Group")).clicked() {
        delete(app, &path, &g.name);
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

fn meta_icon(app: &mut PhotocraftApp, path: &EntryPath, icon: Value) {
    if let Err(e) = app.run(STOCK_META_SET, json!({"path": path.to_string(), "icon": icon})) {
        app.ui.status = e;
    }
}

/// The context menu of a tile (`None`: the empty area of a group).
fn subtool_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, group: &EntryPath, s: Option<(&EntryPath, &str)>) {
    if ui.button(tl!("New Sub Tool…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::NewSubTool { group: group.clone() }, "");
        ui.close();
    }
    let Some((path, name)) = s else { return };
    if ui.button(tl!("Duplicate Sub Tool")).clicked() {
        duplicate(app, path);
        ui.close();
    }
    if ui.button(tl!("Rename Sub Tool…")).clicked() {
        dialog::ask(ui.ctx(), Purpose::Rename { path: path.clone() }, name);
        ui.close();
    }
    if ui.button(tl!("Delete Sub Tool")).clicked() {
        delete(app, path, name);
        ui.close();
    }
    ui.separator();
    if ui.button(tl!("Save Brushes As…")).clicked() {
        if let Err(e) = brushes::export_dialog(app, Some(path.clone())) {
            app.ui.status = e;
        }
        ui.close();
    }
}

/// Duplicate a sub tool in its group (an independent copy, "G-Pen 2") and select the copy.
fn duplicate(app: &mut PhotocraftApp, path: &EntryPath) {
    let to = path.parent().map(|p| p.to_string()).unwrap_or_default();
    let result = app.run(STOCK_COPY, json!({"path": path.to_string(), "to": to})).and_then(|made| {
        match made.get("path").and_then(Value::as_str).and_then(|p| EntryPath::parse(p).ok()) {
            Some(copy) => super::select(app, &copy),
            None => Ok(made),
        }
    });
    if let Err(e) = result {
        app.ui.status = e;
    }
}

/// Move an entry to the trash.
fn delete(app: &mut PhotocraftApp, path: &EntryPath, name: &str) {
    match app.run(STOCK_DELETE, json!({"path": path.to_string()})) {
        Ok(_) => app.ui.status = format!("{name}: {}", tl!("moved to the trash.")),
        Err(e) => app.ui.status = e,
    }
}

/// New, Duplicate and Delete at the bottom right (S6).
fn footer_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: &str, group: Option<&str>) {
    let active = super::active_entry(app).filter(|p| matches!(p, EntryPath::SubTool(t, ..) if t == tool));
    let group_path = group.map(|g| EntryPath::Group(tool.to_owned(), g.to_owned()));
    photocraft_ui_egui::widgets::panel_footer(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let delete_tip = tl!("Delete Sub Tool");
            let del = ui.add_enabled_ui(active.is_some(), |ui| photocraft_ui_egui::icons::button(ui, "trash", 22.0, false, delete_tip)).inner;
            if del.clicked()
                && let Some(p) = &active
            {
                let name = p.name().to_owned();
                delete(app, p, &name);
            }
            let dup = ui.add_enabled_ui(active.is_some(), |ui| photocraft_ui_egui::icons::button(ui, "copy", 22.0, false, tl!("Duplicate Sub Tool"))).inner;
            if dup.clicked()
                && let Some(p) = &active
            {
                duplicate(app, p);
            }
            let new = ui.add_enabled_ui(group_path.is_some(), |ui| photocraft_ui_egui::icons::button(ui, "file-plus", 22.0, false, tl!("New Sub Tool…"))).inner;
            if new.clicked()
                && let Some(g) = &group_path
            {
                dialog::ask(ui.ctx(), Purpose::NewSubTool { group: g.clone() }, "");
            }
        });
    });
}
