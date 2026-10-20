//! Dragging in the palettes (S3, S7, S8). A sub tool tile, a group tab or a tool icon is dragged
//! with the primary button; where it is released decides what happens:
//!
//! | Dragged  | Released on     | Result                                                        |
//! |----------|-----------------|---------------------------------------------------------------|
//! | sub tool | a tile          | into that tile's group, at its place (a reorder in its own)   |
//! | sub tool | a group tab     | into that group, last                                         |
//! | sub tool | a tool icon     | into the group that tool shows, last                          |
//! | group    | a group tab     | into that tab's tool, at its place                            |
//! | group    | a tool icon     | into that tool, last                                          |
//! | tool     | a tool icon     | to that place in the Tool palette                             |
//!
//! Holding Ctrl (Cmd on macOS) at the release copies instead (`clippy.stock.copy`; copies are
//! independent, P5). Released anywhere else, nothing happens; Esc cancels. Every result is one
//! `clippy.stock.move` or `.copy`, and the selection follows a moved sub tool.

use photocraft_clippy_stock::EntryPath;
use photocraft_engine::clippy_cmds::{STOCK_COPY, STOCK_MOVE};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::Tokens;
use serde_json::{Value, json};

use super::Drag;

/// Where a drag can be released.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// A tile: its group and place.
    Tile { group: EntryPath, index: usize },
    /// A group tab: its group and place among the tool's groups.
    Tab { group: EntryPath, index: usize },
    /// A tool icon: its tool and place.
    Tool { tool: String, index: usize },
}

/// Start dragging `payload` when `resp` starts a drag.
pub(crate) fn source(resp: &egui::Response, payload: Drag) {
    if resp.drag_started_by(egui::PointerButton::Primary) {
        egui::DragAndDrop::set_payload(&resp.ctx, payload);
    }
}

/// What is being dragged, if anything.
pub(crate) fn dragging(ctx: &egui::Context) -> Option<Drag> {
    egui::DragAndDrop::payload::<Drag>(ctx).map(|p| (*p).clone())
}

/// Whether `drag` can be released on `target`.
pub(crate) fn accepts(drag: &Drag, target: &Target) -> bool {
    match (drag, target) {
        (Drag::SubTool(_), _) => true,
        (Drag::Group(_), Target::Tab { .. } | Target::Tool { .. }) => true,
        (Drag::Group(_), Target::Tile { .. }) => false,
        (Drag::Tool(_), Target::Tool { .. }) => true,
        _ => false,
    }
}

/// A drop target: highlights `resp`'s rect while an accepted drag hovers it and carries out the
/// drop on release. `true` when something was dropped.
pub(crate) fn target(app: &mut PhotocraftApp, ui: &egui::Ui, resp: &egui::Response, target: Target) -> bool {
    let Some(drag) = dragging(ui.ctx()).filter(|d| accepts(d, &target)) else { return false };
    if !resp.contains_pointer() {
        return false;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(resp.rect.shrink(1.0), 3.0, egui::Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
    if resp.dnd_release_payload::<Drag>().is_none() {
        return false;
    }
    let copy = ui.input(|i| i.modifiers.command);
    if let Err(e) = drop(app, &drag, &target, copy) {
        app.ui.status = e;
    }
    true
}

/// The label of what is dragged, at the pointer (on top of everything).
pub fn ghost(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(drag) = dragging(ctx) else { return };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        egui::DragAndDrop::clear_payload(ctx);
        return;
    }
    let Some(pos) = ctx.pointer_interact_pos() else { return };
    let name = match &drag {
        Drag::SubTool(p) | Drag::Group(p) => p.name().to_owned(),
        Drag::Tool(t) => super::stock(app).and_then(|s| s.tool(t).map(|t| t.name.clone())).unwrap_or_else(|| t.clone()),
    };
    let copy = ctx.input(|i| i.modifiers.command);
    let text = if copy { format!("+ {name}") } else { name };
    let t = Tokens::get(ctx);
    egui::Area::new(egui::Id::new("clippy-palette-drag")).order(egui::Order::Tooltip).fixed_pos(pos + egui::vec2(12.0, 8.0)).interactable(false).show(
        ctx,
        |ui| {
            egui::Frame::NONE.fill(t.card).stroke(egui::Stroke::new(1.0, t.card_border)).corner_radius(3.0).inner_margin(egui::Margin::symmetric(6, 3)).show(
                ui,
                |ui| {
                    ui.label(egui::RichText::new(text).color(t.text));
                },
            );
        },
    );
}

/// Carry out a drop.
pub(crate) fn drop(app: &mut PhotocraftApp, drag: &Drag, target: &Target, copy: bool) -> Result<Value, String> {
    let cmd = if copy { STOCK_COPY } else { STOCK_MOVE };
    let (path, to, index) = match (drag, target) {
        (Drag::SubTool(p), Target::Tile { group, index }) => (p.to_string(), group.to_string(), Some(*index)),
        (Drag::SubTool(p), Target::Tab { group, .. }) => (p.to_string(), group.to_string(), None),
        (Drag::SubTool(p), Target::Tool { tool, .. }) => {
            let group = super::shown_group(app, tool).ok_or_else(|| tl!("This tool has no sub tool group to take the sub tool.").to_string())?;
            (p.to_string(), format!("{tool}/{group}"), None)
        }
        (Drag::Group(p), Target::Tab { group: EntryPath::Group(tool, _), index }) => (p.to_string(), tool.clone(), Some(*index)),
        (Drag::Group(p), Target::Tool { tool, .. }) => (p.to_string(), tool.clone(), None),
        (Drag::Tool(t), Target::Tool { index, .. }) if !copy => (t.clone(), String::new(), Some(*index)),
        _ => return Ok(Value::Null),
    };
    let mut params = json!({"path": path, "to": to});
    if let Some(i) = index {
        params["index"] = json!(i);
    }
    let made = app.run(cmd, params)?;
    // A copied sub tool is the one selected (Clip Studio Paint's Duplicate does the same).
    let made_path = made.get("path").and_then(Value::as_str).and_then(|p| EntryPath::parse(p).ok());
    if copy && let Some(p @ EntryPath::SubTool(..)) = made_path {
        super::select(app, &p)?;
    }
    Ok(made)
}
