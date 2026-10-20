//! The Tool Property palette's controls, after Clip Studio Paint's (S11): a value row (name, a
//! number field and a thin bar under the name), step buttons (anti-aliasing), a five-cell bar
//! (hardness) and check boxes. Colours come from PhotoCraft's theme tokens; the number field and
//! the check box are PhotoCraft's own widgets.

use egui::{Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use photocraft_ui_egui::theme::Tokens;
use photocraft_ui_egui::widgets;

/// Height of the value bar.
const BAR_HEIGHT: f32 = 4.0;
/// Height of a value row.
const ROW_HEIGHT: f32 = 30.0;
/// Width of the number field.
const FIELD_WIDTH: f32 = 58.0;

/// How a value bar maps the pointer to the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scale {
    Linear,
    /// Square root: fine steps at small values (brush sizes up to thousands of pixels).
    Sqrt,
}

/// A value row as in Clip Studio Paint: the name at the top left, a thin bar under it and the
/// number at the right, in one compact row. `true` when the value changed.
pub(crate) fn value_row(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, decimals: u32, scale: Scale, indent: f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let (lo, hi) = (*range.start(), *range.end());
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let field = Rect::from_min_size(pos2(row.right() - FIELD_WIDTH, row.top() + 3.0), vec2(FIELD_WIDTH, ROW_HEIGHT - 6.0));
    let mut changed = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        changed |= widgets::value_field_prec(ui, value, range.clone(), "", FIELD_WIDTH, decimals).changed();
    });
    let left = row.left() + indent;
    let name = ui.painter().layout_no_wrap(tl!(label).to_string(), egui::FontId::proportional(12.0), t.text_dim);
    ui.painter().galley(pos2(left, row.top() + 1.0), name, t.text_dim);
    let bar = Rect::from_min_max(pos2(left + 2.0, row.bottom() - BAR_HEIGHT - 3.0), pos2((field.left() - 10.0).max(left + 40.0), row.bottom() - 3.0));
    let id = ui.id().with(("clippy-value-bar", label));
    let resp = ui.interact(bar.expand2(vec2(0.0, 3.0)), id, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    resp.widget_info(|| egui::WidgetInfo::slider(true, f64::from(*value), tl!(label)));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let f = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        let f = if scale == Scale::Sqrt { f * f } else { f };
        let v = lo + (hi - lo) * f;
        let step = 10f32.powi(-(decimals as i32));
        let v = ((v / step).round() * step).clamp(lo, hi);
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            changed = true;
        }
    }
    let f = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    let f = if scale == Scale::Sqrt { f.sqrt() } else { f };
    let p = ui.painter();
    p.rect_filled(bar, 1.0, t.field);
    p.rect_stroke(bar, 1.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, bar.height())), 1.0, if resp.hovered() || resp.dragged() { t.accent } else { t.text_dim });
    changed
}

/// A row of `count` step buttons, one of them on (`value`); the anti-aliasing steps draw circles
/// from jagged to soft. `true` when another step was chosen.
pub(crate) fn steps(ui: &mut Ui, label: &str, value: &mut u8, count: u8, tips: &[&str]) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!(label)).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for i in (0..count).rev() {
                let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                let on = *value == i;
                let p = ui.painter();
                p.rect_filled(
                    rect,
                    3.0,
                    if on {
                        t.accent_soft
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        Color32::TRANSPARENT
                    },
                );
                if on {
                    p.rect_stroke(rect, 3.0, Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
                }
                soft_dot(p, rect.center(), 6.0, i, count, if on { t.text } else { t.text_dim });
                let tip = tips.get(usize::from(i)).copied().unwrap_or_default();
                let resp = resp.on_hover_text(tl!(tip));
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, tl!(tip)));
                if resp.clicked() && !on {
                    *value = i;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// A row of icon buttons, one of them on (`value`), each with its tooltip (a translation key).
/// `true` when another one was chosen.
pub(crate) fn steps_icons(ui: &mut Ui, label: &str, value: &mut u8, buttons: &[(&str, &str)]) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!(label)).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (i, (icon, tip)) in buttons.iter().enumerate().rev() {
                let i = i as u8;
                if photocraft_ui_egui::icons::button(ui, icon, 22.0, *value == i, tl!(tip)).clicked() && *value != i {
                    *value = i;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// A dot whose edge softens with `level` (0 = jagged … `count - 1` = softest).
fn soft_dot(p: &egui::Painter, c: egui::Pos2, r: f32, level: u8, count: u8, color: Color32) {
    if level == 0 {
        // Jagged: a stepped "pixel" circle.
        for (dx, dy, w, h) in [(-2.0, -6.0, 4.0, 12.0), (-4.0, -5.0, 8.0, 10.0), (-5.0, -4.0, 10.0, 8.0), (-6.0, -2.0, 12.0, 4.0)] {
            p.rect_filled(Rect::from_min_size(c + vec2(dx, dy), vec2(w, h)), 0.0, color);
        }
        return;
    }
    let soft = f32::from(level) / f32::from(count.max(2) - 1);
    let rings = 4;
    for i in (0..=rings).rev() {
        let k = i as f32 / rings as f32;
        let alpha = (1.0 - k * soft).clamp(0.0, 1.0);
        p.circle_filled(c, r * (1.0 - soft * 0.35) + r * soft * 0.35 * k, color.gamma_multiply(alpha * 0.6 + 0.1));
    }
    p.circle_filled(c, r * (1.0 - soft * 0.35), color);
}

/// Five cells filled from the left: `value` in 0..=1 shows as 1–5 cells (Clip Studio Paint's
/// hardness bar). `true` when a cell was chosen.
pub(crate) fn cells(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let filled = ((*value * 5.0).ceil() as i32).clamp(1, 5);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!(label)).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for i in (1..=5).rev() {
                let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 12.0), Sense::click());
                let fill = if i <= filled {
                    if resp.hovered() { t.accent } else { t.text_dim }
                } else if resp.hovered() {
                    t.hover
                } else {
                    t.field
                };
                ui.painter().rect_filled(rect, 1.0, fill);
                ui.painter().rect_stroke(rect, 1.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
                let resp = resp.on_hover_text(format!("{} {i}", tl!(label)));
                if resp.clicked() && i != filled {
                    *value = i as f32 / 5.0;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// A check box row, indented under its parent setting when `indent` is set.
pub(crate) fn check(ui: &mut Ui, label: &str, on: &mut bool, indent: f32) -> Response {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        widgets::checkbox(ui, on, label)
    })
    .inner
}
