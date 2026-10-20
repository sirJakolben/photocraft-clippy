//! Palette headers (features D5, D6): inactive tabs show only their palette's icon, the active one
//! its icon and name, every tab its name as a tooltip; the first tab of a group carries the ☰
//! palette menu at the left of the header; a double click on a tab strip collapses the area; the
//! first group of a dock has the collapse button at the right of its header (D8).
//!
//! craft_ui draws the tabs and their text; the text leaves room (leading space) where this module
//! then paints the icon and the menu button over the tab, on the tab's own layer.

use craft_ui::docking::{DockStyle, Zone};
use photocraft_ui_egui::icons;
use photocraft_ui_egui::theme::Tokens;

use super::panels::PanelId;
use super::{Gesture, Group, tab_id};

/// Room for a tab's icon, points.
const ICON_W: f32 = 16.0;
/// Room for the ☰ button before the first tab's icon.
const MENU_W: f32 = 22.0;
/// The collapse button at the right of a dock's first header.
pub const COLLAPSE_W: f32 = 26.0;

/// A tab's text: room for the ☰ button (`first`) and the icon, then the name when `active`.
pub fn label(panel: PanelId, active: bool, first: bool, font: &egui::FontId, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let lead = ICON_W + if first { MENU_W } else { 0.0 };
    let text = if active { format!(" {}", panel.title()) } else { " ".into() };
    job.append(&text, lead, egui::TextFormat::simple(font.clone(), color));
    job
}

/// The button that collapses a dock to its rail, pointing at the window edge.
pub fn collapse_button(ui: &mut egui::Ui, side: Zone, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let rect = ui.max_rect();
    let response = ui.interact(rect, ui.id().with("collapse-dock"), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let tip = tl!("Collapse Area");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    let tint = if response.hovered() && enabled { t.text } else { t.text_faint };
    icons::paint(ui, rect, toward_edge(side), 14.0, tint);
    response.on_hover_text(tip)
}

/// The chevrons pointing at the window edge a dock sits on.
pub fn toward_edge(side: Zone) -> &'static str {
    match side {
        Zone::Left => "chevrons-left",
        Zone::Right => "chevrons-right",
        Zone::Top => "chevron-up",
        _ => "chevron-down",
    }
}

/// The chevrons pointing away from the window edge (a rail's expand button).
pub fn from_edge(side: Zone) -> &'static str {
    match side {
        Zone::Left => "chevrons-right",
        Zone::Right => "chevrons-left",
        Zone::Top => "chevron-down",
        _ => "chevron-up",
    }
}

/// Icons, tooltips, the ☰ menus and the double click on every drawn group's tab strip.
pub(crate) fn decorate(ui: &mut egui::Ui, groups: &[Group], style: &DockStyle, locked: bool, gestures: &mut Vec<Gesture>) {
    let ctx = ui.ctx().clone();
    for group in groups {
        let Some(strip) = group.strip else { continue };
        let mut right = strip.left();
        let mut layer = None;
        for (i, panel) in group.panels.iter().enumerate() {
            // A tab hidden in the overflow menu has no rect this frame (or an old one elsewhere).
            let Some(tab) = ctx.read_response(tab_id(*panel)).filter(|r| strip.expand(1.0).contains_rect(r.rect) && r.rect.width() > 1.0) else { continue };
            right = right.max(tab.rect.right());
            layer = Some(tab.layer_id);
            let active = *panel == group.active;
            let first = i == 0;
            let color = if active { style.text } else { style.inactive_text };
            let galley = egui::WidgetText::from(label(*panel, active, first, &style.font, color)).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                style.font.clone(),
            );
            // craft_ui centres the text in the tab.
            let left = tab.rect.center().x - galley.size().x * 0.5;
            let mut painter = ui.new_child(egui::UiBuilder::new().layer_id(tab.layer_id).max_rect(tab.rect));
            painter.set_clip_rect(tab.rect.intersect(strip));
            let menu_w = if first { MENU_W } else { 0.0 };
            let icon = egui::Rect::from_min_size(egui::pos2(left + menu_w, tab.rect.top()), egui::vec2(ICON_W, tab.rect.height()));
            icons::paint(&painter, icon, panel.info().icon, 14.0, color);
            if first {
                let menu = egui::Rect::from_min_size(egui::pos2(left.max(tab.rect.left()), tab.rect.top()), egui::vec2(MENU_W, tab.rect.height()));
                palette_menu(&mut painter, menu, group, locked, gestures);
            }
            if tab.double_clicked()
                && let Some(side) = group.side
            {
                gestures.push(Gesture::Collapse(side));
            }
            tab.on_hover_text(panel.title());
        }
        // The empty rest of the strip: a double click there collapses the area too.
        if let (Some(layer), Some(side)) = (layer, group.side) {
            let head = if group.panels.first().is_some_and(|p| is_dock_head(*p, groups, side)) { COLLAPSE_W } else { 0.0 };
            let free = egui::Rect::from_min_max(egui::pos2(right, strip.top()), egui::pos2(strip.right() - head, strip.bottom()));
            if free.width() > 1.0 {
                let ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(free));
                if ui.interact(free, tab_id(group.active).with("strip"), egui::Sense::click()).double_clicked() {
                    gestures.push(Gesture::Collapse(side));
                }
            }
        }
    }
}

/// Whether `panel` heads its dock's first group: the top group of a column, left of a row.
fn is_dock_head(panel: PanelId, groups: &[Group], side: Zone) -> bool {
    groups
        .iter()
        .filter(|g| g.side == Some(side) && !g.floating)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()).then(a.rect.left().total_cmp(&b.rect.left())))
        .is_some_and(|g| g.panels.first() == Some(&panel))
}

/// The ☰ button and its menu, for the group's active palette.
fn palette_menu(ui: &mut egui::Ui, rect: egui::Rect, group: &Group, locked: bool, gestures: &mut Vec<Gesture>) {
    let t = Tokens::get(ui.ctx());
    let response = ui.interact(rect, tab_id(group.active).with("palette-menu"), egui::Sense::click());
    let tip = tl!("Palette Menu");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    let tint = if response.hovered() { t.text } else { t.text_faint };
    let c = rect.center();
    for k in 0..3 {
        let y = c.y - 3.5 + k as f32 * 3.5;
        ui.painter().line_segment([egui::pos2(c.x - 5.0, y), egui::pos2(c.x + 5.0, y)], egui::Stroke::new(1.0, tint));
    }
    let response = response.on_hover_text(tip);
    let panel = group.active;
    egui::Popup::menu(&response).show(|ui| {
        photocraft_ui_egui::widgets::style_spectrum_popup_menu(ui);
        if ui.add_enabled(!locked, egui::Button::new(tl!("Close Palette"))).clicked() {
            gestures.push(Gesture::Close(panel));
            ui.close();
        }
        if let Some(side) = group.side
            && ui.add_enabled(!locked, egui::Button::new(tl!("Collapse Area"))).clicked()
        {
            gestures.push(Gesture::Collapse(side));
            ui.close();
        }
        if !group.floating && ui.add_enabled(!locked, egui::Button::new(tl!("Float Palette"))).clicked() {
            gestures.push(Gesture::Float(panel));
            ui.close();
        }
        ui.separator();
        if ui.button(tl!("Reset Clippy Workspace")).clicked() {
            gestures.push(Gesture::Reset);
            ui.close();
        }
    });
}
