//! Clippy's docks (features D1–D3, D10, D13, D14): the window under the menu is one docking
//! layout with the canvas in the middle and palettes on every side, built on
//! `craft_ui::docking` as PhotoCraft's own dock is.
//!
//! The layout lives in the preference `clippy.layouts.clippy`, written only through the
//! `clippy.layout.*` commands, so agents and the control channel see and change it like the user.
//! While the pointer is down (a split or a floating window being dragged) the frame shows a live
//! copy; releasing it saves once.

pub mod layout;
pub mod panels;

use craft_ui::docking::{Action, DockArea, DockContent, DockStyle, PanelLimits, Permissions};
use photocraft_engine::clippy_cmds::{LAYOUT_KEY, LAYOUT_RESET, LAYOUT_SET};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::menus::{self, MenuItem};
use photocraft_ui_egui::theme::Tokens;
use serde_json::{Value, json};

use layout::DockLayout;
use panels::{MIN_WIDTH, PanelId};

/// The egui id of the dock area; its temp data holds the live layout.
fn area_id() -> egui::Id {
    egui::Id::new("clippy-dock")
}

/// The layout the frame shows, the saved value it came from, and whether it changed since.
#[derive(Clone)]
struct Live {
    saved: Value,
    layout: DockLayout,
    dirty: bool,
}

/// The saved layout value (null when there is none).
pub fn saved(app: &PhotocraftApp) -> Value {
    app.session.prefs().clippy.layouts.get(LAYOUT_KEY).cloned().unwrap_or(Value::Null)
}

/// The layout in effect: the saved one, or the default when it is missing or unreadable.
pub fn current(app: &PhotocraftApp) -> DockLayout {
    layout::read(&saved(app)).unwrap_or_else(|_| layout::default_layout())
}

/// Draw the docks into `ui` (the window below the menu and option bars).
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let mut live = live(app, &ctx);
    let t = Tokens::get(&ctx);
    let locked = app.session.prefs().workspace_locked;
    let permissions = move |p: &PanelId| permissions(*p, locked);
    let output = DockArea::new(area_id()).show_customized(ui, &live.layout, &style(ui, &t), |p| p.title(), permissions, limits, &mut Content { app });
    if let Some(error) = output.error {
        app.ui.status = error.to_string();
    }
    for action in output.actions {
        live.dirty |= apply(&mut live.layout, action, locked);
    }
    if live.dirty && !ctx.input(|i| i.pointer.any_down()) {
        live.dirty = false;
        let value = serde_json::to_value(&live.layout).unwrap_or(Value::Null);
        match app.run(LAYOUT_SET, json!({"layout": value})) {
            Ok(_) => live.saved = value,
            Err(e) => app.ui.status = e,
        }
    }
    ctx.data_mut(|d| d.insert_temp(area_id(), live));
}

/// Window › Interface › Reset Clippy Workspace, after the interface switch.
pub(crate) fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    let item = MenuItem {
        id: LAYOUT_RESET.into(),
        label: "Reset Clippy Workspace".into(),
        path: vec!["Window".into(), "Interface".into()],
        shortcut: None,
        enabled: menus::is_enabled(app, LAYOUT_RESET),
        checked: None,
        color: None,
    };
    let at = items.iter().rposition(|i| i.path == item.path).map_or(items.len(), |p| p + 1);
    items.insert(at, item);
}

/// Apply one dock change to `layout` when Clippy's rules and the panels' permissions allow it.
/// Returns whether it changed the layout.
pub fn apply(layout: &mut DockLayout, action: Action<PanelId>, locked: bool) -> bool {
    if !layout::allowed(layout, &action) {
        return false;
    }
    let mut next = layout.clone();
    if next.apply_with_permissions(action, |p| permissions(*p, locked)).is_err() || layout::check(&next).is_err() {
        return false;
    }
    let changed = next != *layout;
    *layout = next;
    changed
}

/// The live copy, reloaded whenever the saved layout changed underneath (a command, an agent,
/// a reset).
fn live(app: &mut PhotocraftApp, ctx: &egui::Context) -> Live {
    let saved = saved(app);
    if let Some(live) = ctx.data(|d| d.get_temp::<Live>(area_id())).filter(|l| l.saved == saved) {
        return live;
    }
    let layout = match layout::read(&saved) {
        Ok(layout) => layout,
        Err(error) => {
            log::warn!("clippy dock: the saved layout can't be shown ({error:?}); using the default");
            app.ui.status = tl!("The saved Clippy layout could not be read; the default layout is shown.").into();
            layout::default_layout()
        }
    };
    Live { saved, layout, dirty: false }
}

fn permissions(panel: PanelId, locked: bool) -> Permissions {
    if panel == PanelId::Canvas {
        Permissions::PROTECTED
    } else {
        Permissions { movable: !locked, floatable: !locked, closable: !locked, accepts_tabs: !locked }
    }
}

fn limits(panel: &PanelId) -> PanelLimits {
    if *panel == PanelId::Canvas {
        PanelLimits { min: egui::vec2(200.0, 200.0), ..Default::default() }
    } else {
        PanelLimits { min: egui::vec2(MIN_WIDTH, panel.info().min_height), ..Default::default() }
    }
}

/// PhotoCraft's dock look, so palettes read the same in both interfaces.
fn style(ui: &egui::Ui, t: &Tokens) -> DockStyle {
    let mut style = DockStyle::from_ui(ui);
    style.tab_height = if t.pro { 28.0 } else { 34.0 };
    style.background = t.card;
    style.tab_background = if t.tab_strip.a() == 0 { t.card } else { t.tab_strip };
    style.active_background = t.card;
    style.text = t.text;
    style.inactive_text = t.text_dim;
    style.border = egui::Stroke::new(1.0, t.card_border);
    style.accent = t.accent;
    style.float_label = tl!("Float Panel").to_string();
    style.close_label = tl!("Close Panel").to_string();
    style.move_label = tl!("Group with").to_string();
    style.panels_label = tl!("Panels").to_string();
    style.resize_label = tl!("Resize panels").to_string();
    style.resize_window_label = tl!("Resize panel window").to_string();
    style
}

struct Content<'a> {
    app: &'a mut PhotocraftApp,
}

impl DockContent<PanelId> for Content<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &PanelId) {
        panels::body(self.app, ui, *panel);
    }
}
