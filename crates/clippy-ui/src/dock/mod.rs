//! Clippy's docks (features D1–D10, D13, D14): the window under the menu is one docking
//! layout with the canvas in the middle and palettes on every side, built on
//! `craft_ui::docking` as PhotoCraft's own dock is.
//!
//! The layout lives in the preference `clippy.layouts.clippy`, written only through the
//! `clippy.layout.*` and `clippy.workspace.*` commands, so agents and the control channel see and
//! change it like the user. While the pointer is down (a split or a floating window being dragged)
//! the frame shows a live copy; releasing it saves once.
//!
//! Around craft_ui's dock area (M3): tabs show icons, the active one its name too ([`header`]);
//! each group has a ☰ menu at the left of its header; a dock collapses to an icon [`rail`]; the
//! drop preview never offers a drop the D14 rules refuse; Window lists every palette and
//! Window › Workspace saves and loads layouts ([`workspace`]).

pub mod header;
pub mod layout;
pub mod panels;
pub mod rail;
pub mod workspace;

use craft_ui::docking::{Action, DockArea, DockContent, DockStyle, PanelLimits, Permissions, Zone, drop_zone};
use photocraft_engine::clippy_cmds::{LAYOUT_KEY, LAYOUT_RESET, LAYOUT_SET};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::menus::{self, MenuItem};
use photocraft_ui_egui::theme::Tokens;
use serde_json::{Value, json};

use layout::DockLayout;
use panels::{MIN_WIDTH, PANELS, PanelId};

/// The menu id of a palette in Window: `clippy.palette.<id>` shows or closes it.
pub const PALETTE_PREFIX: &str = "clippy.palette.";

/// PhotoCraft's Window items whose panels Clippy's palettes replace; in Clippy mode the palettes
/// take their place.
const REPLACED_WINDOW_ITEMS: &[&str] = &[
    "window.panel.actions",
    "window.panel.adjustments",
    "window.panel.brushes",
    "window.panel.channels",
    "window.panel.character",
    "window.panel.color",
    "window.panel.gradients",
    "window.panel.histogram",
    "window.panel.history",
    "window.panel.info",
    "window.panel.layerComps",
    "window.panel.layers",
    "window.panel.navigator",
    "window.panel.paragraph",
    "window.panel.paths",
    "window.panel.patterns",
    "window.panel.properties",
    "window.panel.swatches",
    "window.panel.tools",
];

/// The egui id of the dock area; its temp data holds the live layout.
pub fn area_id() -> egui::Id {
    egui::Id::new("clippy-dock")
}

/// The egui id of a palette's tab, as craft_ui names it.
pub fn tab_id(panel: PanelId) -> egui::Id {
    area_id().with(("tab", panel))
}

/// The layout the frame shows, the saved value it came from, and whether it changed since.
#[derive(Clone)]
struct Live {
    saved: Value,
    layout: DockLayout,
    collapsed: Vec<Zone>,
    dirty: bool,
}

/// One drawn tab group of the last frame, for the drop preview and the headers.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub panels: Vec<PanelId>,
    pub active: PanelId,
    /// The whole group; its tab strip is the top `tab_height` unless it is the bare canvas.
    pub rect: egui::Rect,
    pub strip: Option<egui::Rect>,
    pub floating: bool,
    /// The side of the dock holding it.
    pub side: Option<Zone>,
}

/// A change asked for by Clippy's own controls (headers, menus, rails), applied after the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Gesture {
    Close(PanelId),
    Float(PanelId),
    Collapse(Zone),
    Expand(Zone),
    Reset,
}

/// The saved layout value (null when there is none).
pub fn saved(app: &PhotocraftApp) -> Value {
    app.session.prefs().clippy.layouts.get(LAYOUT_KEY).cloned().unwrap_or(Value::Null)
}

/// The layout in effect: the saved one, or the default when it is missing or unreadable.
pub fn current(app: &PhotocraftApp) -> DockLayout {
    layout::read(&saved(app)).unwrap_or_else(|_| layout::default_layout())
}

/// The sides whose dock is collapsed to an icon rail.
pub fn collapsed(app: &PhotocraftApp) -> Vec<Zone> {
    let saved = saved(app);
    match layout::read(&saved) {
        Ok(layout) => layout::read_collapsed(&saved, &layout),
        Err(_) => Vec::new(),
    }
}

/// Whether the last frame held back the drop preview: a tab dragged where the D14 rules refuse it.
pub fn drop_refused(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(area_id().with("refused"))).unwrap_or(false)
}

/// Draw the docks into `ui` (the window below the menu and option bars).
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let mut live = live(app, &ctx);
    let t = Tokens::get(&ctx);
    let locked = app.session.prefs().workspace_locked;
    let style = style(ui, &t);
    let view = layout::view(&live.layout, &live.collapsed);
    let (dock_rect, rails) = rail::carve(ui.available_rect_before_wrap(), &view.rails);

    // The drop preview: craft_ui offers every edge of a group; where Clippy's rules refuse the
    // drop, the pointer is a blocked spot, so neither preview nor drop happens.
    let last: Vec<Group> = ctx.data(|d| d.get_temp(area_id().with("groups"))).unwrap_or_default();
    let refused =
        dragged_panel(&ctx, &live.layout).zip(ctx.pointer_interact_pos()).is_some_and(|(panel, pointer)| refuses(&live.layout, panel, pointer, &last));
    let pointer = ctx.pointer_interact_pos().filter(|_| refused);
    let blocked: Vec<egui::Rect> = pointer.map(|p| egui::Rect::from_center_size(p, egui::vec2(2.0, 2.0))).into_iter().collect();

    let mut content = Content::new(app, &view.layout, &live.layout);
    let permissions = move |p: &PanelId| permissions(*p, locked);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(dock_rect));
    let output =
        DockArea::new(area_id()).blocked_rects(&blocked).show_customized(&mut child, &view.layout, &style, |p| p.title(), permissions, limits, &mut content);
    let Content { bodies, mut gestures, .. } = content;
    if let Some(error) = output.error {
        app.ui.status = error.to_string();
    }
    let groups = drawn_groups(&view.layout, &live.layout, &bodies, style.tab_height);
    header::decorate(ui, &groups, &style, locked, &mut gestures);
    rail::show(app, ui, &rails, locked, &mut gestures);
    workspace::dialogs(app, &ctx);
    crate::palettes::dialogs(app, &ctx);
    crate::palettes::drag_ghost(app, &ctx);

    for action in output.actions {
        live.dirty |= apply(&mut live.layout, view.real_action(action), locked);
    }
    for gesture in gestures {
        if gesture == Gesture::Reset {
            if let Err(e) = app.run(LAYOUT_RESET, json!({})) {
                app.ui.status = e;
            }
            ctx.data_mut(|d| d.remove::<Live>(area_id()));
            return;
        }
        live.dirty |= apply_gesture(&mut live, gesture, &groups, locked, ctx.content_rect());
    }
    let sides = layout::sides(&live.layout);
    live.collapsed.retain(|s| sides.contains(s));
    if live.dirty && !ctx.input(|i| i.pointer.any_down()) {
        live.dirty = false;
        let value = layout::to_value(&live.layout, &live.collapsed);
        match app.run(LAYOUT_SET, json!({"layout": value})) {
            Ok(_) => live.saved = value,
            Err(e) => app.ui.status = e,
        }
    }
    ctx.data_mut(|d| {
        d.insert_temp(area_id(), live);
        d.insert_temp(area_id().with("groups"), groups);
        d.insert_temp(area_id().with("refused"), refused);
    });
}

/// Window: one item per palette in Clippy mode (checked while shown) in place of PhotoCraft's
/// panels, and Window › Interface › Reset Clippy Workspace after the interface switch.
pub(crate) fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    if crate::mode::active(app) {
        let shown = current(app);
        let locked = app.session.prefs().workspace_locked;
        let palettes = PANELS.iter().filter(|p| p.id != PanelId::Canvas).map(|p| MenuItem {
            id: palette_item(p.id),
            label: p.title.into(),
            path: vec!["Window".into()],
            shortcut: None,
            enabled: !locked,
            checked: Some(shown.contains(&p.id)),
            color: None,
        });
        let at = items.iter().position(|i| REPLACED_WINDOW_ITEMS.contains(&i.id.as_str()));
        items.retain(|i| !REPLACED_WINDOW_ITEMS.contains(&i.id.as_str()));
        let at = at.unwrap_or(items.len()).min(items.len());
        items.splice(at..at, palettes);
    }
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

/// The Window item of `panel`.
pub fn palette_item(panel: PanelId) -> String {
    format!("{PALETTE_PREFIX}{}", serde_json::to_value(panel).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default())
}

/// Window › a palette: close it when shown (docked, floating or in a rail), else show it again
/// beside its default group. Saved through `clippy.layout.set`.
pub fn toggle_palette(app: &mut PhotocraftApp, ctx: &egui::Context, panel: PanelId) -> Result<Value, String> {
    if panel == PanelId::Canvas {
        return Err("the canvas is always shown".into());
    }
    if app.session.prefs().workspace_locked {
        return Err(tl!("The workspace is locked.").into());
    }
    let saved = saved(app);
    let mut layout = current(app);
    let collapsed = layout::read_collapsed(&saved, &layout);
    let shown = layout.contains(&panel);
    if shown {
        if !apply(&mut layout, Action::Close { panel }, false) {
            return Err(format!("{} can't be closed", panel.title()));
        }
    } else {
        let screen = ctx.content_rect();
        let size = egui::vec2(260.0, 300.0);
        let min = screen.center() - size * 0.5;
        layout = layout::reopened(&layout, panel, [min.x, min.y, size.x, size.y]).ok_or_else(|| format!("{} can't be shown", panel.title()))?;
    }
    let sides = layout::sides(&layout);
    let collapsed: Vec<Zone> = collapsed.into_iter().filter(|s| sides.contains(s)).collect();
    app.run(LAYOUT_SET, json!({"layout": layout::to_value(&layout, &collapsed)}))?;
    Ok(json!({"panel": panel, "shown": !shown}))
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

fn apply_gesture(live: &mut Live, gesture: Gesture, groups: &[Group], locked: bool, screen: egui::Rect) -> bool {
    match gesture {
        Gesture::Close(panel) => apply(&mut live.layout, Action::Close { panel }, locked),
        Gesture::Float(panel) => {
            let rect =
                groups.iter().find(|g| g.panels.contains(&panel)).map_or(egui::Rect::from_min_size(screen.center(), egui::vec2(260.0, 300.0)), |g| g.rect);
            let size = rect.size().min(screen.size());
            let min = (rect.min + egui::vec2(24.0, 24.0)).min(screen.max - size).max(screen.min);
            apply(&mut live.layout, Action::Float { panel, rect: [min.x, min.y, size.x, size.y] }, locked)
        }
        Gesture::Collapse(side) if !locked && !live.collapsed.contains(&side) && layout::sides(&live.layout).contains(&side) => {
            live.collapsed.push(side);
            true
        }
        Gesture::Expand(side) if !locked && live.collapsed.contains(&side) => {
            live.collapsed.retain(|s| *s != side);
            true
        }
        _ => false,
    }
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
    let collapsed = layout::read_collapsed(&saved, &layout);
    Live { saved, layout, collapsed, dirty: false }
}

/// The palette whose tab is being dragged (or was released this frame).
fn dragged_panel(ctx: &egui::Context, layout: &DockLayout) -> Option<PanelId> {
    let id = ctx.dragged_id().or_else(|| ctx.drag_stopped_id())?;
    layout.panels().into_iter().copied().find(|p| tab_id(*p) == id)
}

/// Whether dropping `panel` at `pointer` is a split Clippy's rules refuse. `groups` are the drawn
/// groups back to front, as craft_ui resolves a drop: the frontmost group under the pointer
/// decides, its tab strip and middle are tab drops (always fine), its edges splits.
fn refuses(layout: &DockLayout, panel: PanelId, pointer: egui::Pos2, groups: &[Group]) -> bool {
    let Some(target) = groups.iter().rev().find(|g| g.rect.contains(pointer)) else { return false };
    if target.floating || target.strip.is_some_and(|s| s.contains(pointer)) {
        return false;
    }
    let Some(zone) = drop_zone(target.rect, 0.0, pointer).filter(|z| *z != Zone::Center) else { return false };
    let anchor = if target.active == panel { target.panels.iter().copied().find(|p| *p != panel) } else { Some(target.active) };
    let Some(anchor) = anchor else { return false };
    !layout::allowed(layout, &Action::Move { panel, anchor, placement: craft_ui::docking::Placement::Split(zone) })
}

/// The groups drawn this frame, back to front, from the bodies craft_ui drew (each group draws
/// its active palette's body below its tab strip).
fn drawn_groups(view: &DockLayout, full: &DockLayout, bodies: &[(PanelId, egui::Rect)], tab_height: f32) -> Vec<Group> {
    let docked = view.root.as_ref().map(layout::groups).unwrap_or_default();
    let mut all: Vec<(Vec<PanelId>, bool)> = docked.into_iter().map(|g| (g, false)).collect();
    all.extend(view.floating.iter().map(|f| (f.panels.clone(), true)));
    let mut groups = Vec::new();
    for (panel, body) in bodies {
        let Some((panels, floating)) = all.iter().find(|(g, _)| g.contains(panel)) else { continue };
        let bare = *panel == PanelId::Canvas;
        let rect = if bare { *body } else { egui::Rect::from_min_max(egui::pos2(body.left(), body.top() - tab_height), body.max) };
        let strip = (!bare).then(|| egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), body.top())));
        let side = if *floating { None } else { layout::dock_side(full, *panel) };
        groups.push(Group { panels: panels.clone(), active: *panel, rect, strip, floating: *floating, side });
    }
    groups
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

/// craft_ui's adapter: bodies, tab labels (D5), the tab menu and the collapse button (D8).
struct Content<'a> {
    app: &'a mut PhotocraftApp,
    /// The active palette of every group.
    active: Vec<PanelId>,
    /// The first palette of every group, whose tab carries the ☰ menu.
    first: Vec<PanelId>,
    /// The first palette of each dock's first group, and the dock's side: its header carries the
    /// collapse button.
    dock_heads: Vec<(PanelId, Zone)>,
    /// Docked palettes and their dock's side.
    sides: Vec<(PanelId, Zone)>,
    /// The bodies drawn, in craft_ui's order (docked, then floating back to front).
    bodies: Vec<(PanelId, egui::Rect)>,
    gestures: Vec<Gesture>,
}

impl<'a> Content<'a> {
    fn new(app: &'a mut PhotocraftApp, view: &DockLayout, full: &DockLayout) -> Self {
        let mut active = Vec::new();
        let mut first = Vec::new();
        let mut pending: Vec<&craft_ui::docking::Node<PanelId>> = view.root.iter().collect();
        while let Some(node) = pending.pop() {
            match node {
                craft_ui::docking::Node::Split { first, second, .. } => pending.extend([first.as_ref(), second.as_ref()]),
                craft_ui::docking::Node::Tabs { panels, active: a } => {
                    active.extend(panels.get(*a).copied());
                    first.extend(panels.first().copied());
                }
                craft_ui::docking::Node::Stack { .. } => {}
            }
        }
        for f in &view.floating {
            active.extend(f.panels.get(f.active).copied());
            first.extend(f.panels.first().copied());
        }
        let docks = layout::docks(full);
        let dock_heads = docks.iter().filter_map(|d| d.groups.first().and_then(|g| g.first()).map(|p| (*p, d.side))).collect();
        let sides = docks.iter().flat_map(|d| d.groups.iter().flatten().map(move |p| (*p, d.side))).collect();
        Self { app, active, first, dock_heads, sides, bodies: Vec::new(), gestures: Vec::new() }
    }

    fn side(&self, panel: PanelId) -> Option<Zone> {
        self.sides.iter().find(|(p, _)| *p == panel).map(|(_, s)| *s)
    }
}

impl DockContent<PanelId> for Content<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &PanelId) {
        self.bodies.push((*panel, ui.max_rect()));
        panels::body(self.app, ui, *panel);
    }

    fn label(&mut self, _ui: &egui::Ui, panel: &PanelId, _logical: &str, font: &egui::FontId, color: egui::Color32) -> egui::WidgetText {
        header::label(*panel, self.active.contains(panel), self.first.contains(panel), font, color).into()
    }

    fn panel_menu(&mut self, ui: &mut egui::Ui, panel: &PanelId) {
        let locked = self.app.session.prefs().workspace_locked;
        if let Some(side) = self.side(*panel)
            && ui.add_enabled(!locked, egui::Button::new(tl!("Collapse Area"))).clicked()
        {
            self.gestures.push(Gesture::Collapse(side));
            ui.close();
        }
    }

    fn group_header_width(&self, panels: &[PanelId], _active: &PanelId, floating: bool) -> f32 {
        let head = panels.first().is_some_and(|p| self.dock_heads.iter().any(|(h, _)| h == p));
        if !floating && head { header::COLLAPSE_W } else { 0.0 }
    }

    fn group_header(&mut self, ui: &mut egui::Ui, panels: &[PanelId], _active: &PanelId, floating: bool) {
        let Some(side) = panels.first().and_then(|p| self.dock_heads.iter().find(|(h, _)| h == p)).map(|(_, s)| *s) else { return };
        if !floating && header::collapse_button(ui, side, !self.app.session.prefs().workspace_locked).clicked() {
            self.gestures.push(Gesture::Collapse(side));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(panels: &[PanelId], rect: egui::Rect, floating: bool) -> Group {
        let strip = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 28.0));
        Group { panels: panels.to_vec(), active: panels[0], rect, strip: Some(strip), floating, side: None }
    }

    #[test]
    fn only_refused_splits_hold_the_preview_back() {
        let l = layout::default_layout();
        let layers = egui::Rect::from_min_size(egui::pos2(1000.0, 200.0), egui::vec2(260.0, 300.0));
        let canvas = Group {
            panels: vec![PanelId::Canvas],
            active: PanelId::Canvas,
            rect: egui::Rect::from_min_max(egui::pos2(290.0, 40.0), egui::pos2(990.0, 780.0)),
            strip: None,
            floating: false,
            side: None,
        };
        let groups = [canvas, group(&[PanelId::Layer, PanelId::History], layers, false)];
        // Beside Layers (a column): refused; above, below, in the middle and on the strip: fine.
        assert!(refuses(&l, PanelId::Info, egui::pos2(1005.0, 350.0), &groups));
        assert!(refuses(&l, PanelId::Info, egui::pos2(1255.0, 350.0), &groups));
        assert!(!refuses(&l, PanelId::Info, egui::pos2(1130.0, 495.0), &groups));
        assert!(!refuses(&l, PanelId::Info, egui::pos2(1130.0, 350.0), &groups));
        assert!(!refuses(&l, PanelId::Info, egui::pos2(1005.0, 210.0), &groups));
        // The canvas: a new top or bottom dock is fine, a second right dock is not.
        assert!(!refuses(&l, PanelId::Info, egui::pos2(640.0, 770.0), &groups));
        assert!(refuses(&l, PanelId::Info, egui::pos2(985.0, 400.0), &groups));
        // Outside every group (a float) and over a floating group (tabs only): fine.
        assert!(!refuses(&l, PanelId::Info, egui::pos2(5.0, 5.0), &groups));
        let floating = [group(&[PanelId::Layer], layers, true)];
        assert!(!refuses(&l, PanelId::Info, egui::pos2(1005.0, 350.0), &floating));
        // A palette alone in its group has nothing to split beside.
        let alone = [group(&[PanelId::Info], layers, false)];
        assert!(!refuses(&l, PanelId::Info, egui::pos2(1005.0, 350.0), &alone));
    }

    #[test]
    fn palette_items_name_every_palette_but_the_canvas() {
        assert_eq!(palette_item(PanelId::SubTool), "clippy.palette.subTool");
        assert_eq!(palette_item(PanelId::Layer), "clippy.palette.layer");
    }
}
