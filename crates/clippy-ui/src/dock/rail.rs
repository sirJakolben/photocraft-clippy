//! Docks collapsed to an icon rail (feature D8), as in Clip Studio Paint: the dock's palettes as
//! a column (left, right) or row (top, bottom) of icons along the window edge, with the expand
//! button first. Clicking an icon opens that palette as an overlay beside the rail; a click
//! elsewhere, Esc or the icon again closes it. Whether a dock is collapsed is part of the saved
//! layout (`layout::COLLAPSED_KEY`), so it survives a restart and travels with a workspace.

use craft_ui::docking::Zone;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::icons;
use photocraft_ui_egui::theme::Tokens;

use super::layout::Dock;
use super::panels::{self, MIN_WIDTH, PanelId};
use super::{Gesture, area_id};

/// The rail's thickness, points.
pub const THICKNESS: f32 = 36.0;
/// An icon button in the rail.
const BUTTON: f32 = 28.0;

fn overlay_id() -> egui::Id {
    area_id().with("rail-overlay")
}

/// The palette open from a rail, if any.
pub fn overlay(ctx: &egui::Context) -> Option<PanelId> {
    ctx.data(|d| d.get_temp::<Option<PanelId>>(overlay_id())).flatten()
}

fn set_overlay(ctx: &egui::Context, panel: Option<PanelId>) {
    ctx.data_mut(|d| d.insert_temp(overlay_id(), panel));
}

/// Cut a rail for every collapsed dock from the window's edges, outermost first; the rest is for
/// the dock area.
pub(crate) fn carve(mut rect: egui::Rect, rails: &[Dock]) -> (egui::Rect, Vec<(Dock, egui::Rect)>) {
    let mut out = Vec::new();
    for dock in rails {
        let t = THICKNESS.min(
            match dock.side {
                Zone::Left | Zone::Right => rect.width(),
                _ => rect.height(),
            } * 0.5,
        );
        let rail = match dock.side {
            Zone::Left => {
                let r = egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + t, rect.bottom()));
                rect.min.x += t;
                r
            }
            Zone::Right => {
                let r = egui::Rect::from_min_max(egui::pos2(rect.right() - t, rect.top()), rect.max);
                rect.max.x -= t;
                r
            }
            Zone::Top => {
                let r = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + t));
                rect.min.y += t;
                r
            }
            _ => {
                let r = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - t), rect.max);
                rect.max.y -= t;
                r
            }
        };
        out.push((dock.clone(), rail));
    }
    (rect, out)
}

/// Draw the rails and the open overlay.
pub(crate) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, rails: &[(Dock, egui::Rect)], locked: bool, gestures: &mut Vec<Gesture>) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let mut open = overlay(&ctx).filter(|p| rails.iter().any(|(d, _)| d.groups.iter().any(|g| g.contains(p))));
    let mut anchor = None;
    for (dock, rect) in rails {
        ui.painter().rect_filled(*rect, 0.0, t.chrome);
        ui.painter().rect_stroke(*rect, 0.0, egui::Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
        let vertical = matches!(dock.side, Zone::Left | Zone::Right);
        let layout = if vertical { egui::Layout::top_down(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
        let mut rail = ui.new_child(egui::UiBuilder::new().id_salt(("clippy-rail", side_name(dock.side))).max_rect(rect.shrink(4.0)).layout(layout));
        rail.set_clip_rect(rect.intersect(ui.clip_rect()));
        rail.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        let expand = rail.add_enabled_ui(!locked, |ui| icons::rail_button(ui, super::header::from_edge(dock.side), BUTTON, false, "Expand Area")).inner;
        if expand.clicked() {
            gestures.push(Gesture::Expand(dock.side));
            open = None;
        }
        for group in &dock.groups {
            separator(&mut rail, vertical, &t);
            for panel in group {
                let on = open == Some(*panel);
                let button = icons::rail_button(&mut rail, panel.info().icon, BUTTON, on, panel.info().title);
                if button.clicked() {
                    open = if on { None } else { Some(*panel) };
                }
                if open == Some(*panel) {
                    anchor = Some((dock.clone(), *rect, button.rect));
                }
            }
        }
    }
    let Some((panel, (dock, rail, button))) = open.zip(anchor) else {
        set_overlay(&ctx, None);
        return;
    };
    let shown = overlay_window(app, &ctx, panel, &dock, rail, button);
    set_overlay(&ctx, shown.then_some(panel));
}

fn separator(ui: &mut egui::Ui, vertical: bool, t: &Tokens) {
    let size = if vertical { egui::vec2(BUTTON, 5.0) } else { egui::vec2(5.0, BUTTON) };
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let stroke = egui::Stroke::new(1.0, t.card_border);
    if vertical {
        ui.painter().hline(rect.x_range(), rect.center().y, stroke);
    } else {
        ui.painter().vline(rect.center().x, rect.y_range(), stroke);
    }
}

fn side_name(side: Zone) -> &'static str {
    match side {
        Zone::Left => "left",
        Zone::Right => "right",
        Zone::Top => "top",
        Zone::Bottom => "bottom",
        Zone::Center => "center",
    }
}

/// The palette beside its rail, the dock's width wide. False when it closed this frame.
fn overlay_window(app: &mut PhotocraftApp, ctx: &egui::Context, panel: PanelId, dock: &Dock, rail: egui::Rect, button: egui::Rect) -> bool {
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let extent = dock.extent.unwrap_or(260.0).max(MIN_WIDTH);
    let size = match dock.side {
        Zone::Left | Zone::Right => egui::vec2(extent, (screen.height() * 0.6).clamp(panel.info().min_height + 40.0, 520.0)),
        _ => egui::vec2(320.0f32.max(MIN_WIDTH), extent.clamp(panel.info().min_height + 40.0, 520.0)),
    }
    .min(screen.size());
    let min = match dock.side {
        Zone::Left => egui::pos2(rail.right() + 2.0, button.top()),
        Zone::Right => egui::pos2(rail.left() - 2.0 - size.x, button.top()),
        Zone::Top => egui::pos2(button.left(), rail.bottom() + 2.0),
        _ => egui::pos2(button.left(), rail.top() - 2.0 - size.y),
    };
    let min = min.min(screen.max - size).max(screen.min);
    let rect = egui::Rect::from_min_size(min, size);
    let mut keep = true;
    let area = egui::Area::new(overlay_id().with(panel)).order(egui::Order::Foreground).fixed_pos(rect.min).constrain_to(screen);
    let response = area.show(ctx, |ui| {
        ui.set_min_size(size);
        ui.set_max_size(size);
        ui.painter().rect_filled(rect, t.radius_sm, t.card);
        ui.painter().rect_stroke(rect, t.radius_sm, egui::Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
        let title = egui::Rect::from_min_size(rect.min, egui::vec2(size.x, 28.0));
        icons::paint(ui, egui::Rect::from_min_size(title.min + egui::vec2(8.0, 0.0), egui::vec2(16.0, title.height())), panel.info().icon, 14.0, t.text);
        ui.painter().text(title.left_center() + egui::vec2(30.0, 0.0), egui::Align2::LEFT_CENTER, panel.title(), egui::FontId::proportional(12.5), t.text);
        let close = egui::Rect::from_min_size(egui::pos2(title.right() - 26.0, title.top()), egui::vec2(26.0, title.height()));
        let x = ui.interact(close, overlay_id().with("close"), egui::Sense::click());
        x.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Close")));
        icons::paint(ui, close, "x", 14.0, if x.hovered() { t.text } else { t.text_faint });
        if x.clicked() {
            keep = false;
        }
        let body = egui::Rect::from_min_max(egui::pos2(rect.left(), title.bottom()), rect.max).shrink(1.0);
        let mut child =
            ui.new_child(egui::UiBuilder::new().id_salt(("clippy-rail-body", panel)).max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(body);
        panels::body(app, &mut child, panel);
        ui.allocate_rect(rect, egui::Sense::hover());
    });
    // Esc, or a press on the docks or the canvas (not on a popup, the rail or the overlay).
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside = ctx.input(|i| i.pointer.any_pressed().then_some(i.pointer.interact_pos()).flatten()).is_some_and(|p| {
        !rail.contains(p)
            && !response.response.rect.contains(p)
            && ctx.layer_id_at(p).is_none_or(|l| matches!(l.order, egui::Order::Background | egui::Order::Middle))
    });
    keep && !escape && !outside
}
