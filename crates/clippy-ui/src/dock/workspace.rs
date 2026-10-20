//! Workspaces (feature D9): Window › Workspace in Clippy mode lists the saved workspaces (a click
//! loads one), then New Workspace… and Delete Workspace…, which ask for a name. Every change goes
//! through `clippy.workspace.*` (`photocraft_engine::clippy_cmds`), which keeps one
//! `<name>.json` per workspace in `<settings folder>/workspaces`. The loaded layout is the current
//! one, so it is the one shown after a restart.

use photocraft_engine::clippy_cmds::{WORKSPACE_DELETE, WORKSPACE_LOAD, WORKSPACE_SAVE, workspace_names};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::menus::MenuItem;
use photocraft_ui_egui::theme::Tokens;
use photocraft_ui_egui::widgets::{self, ButtonRole, DialogButton};
use serde_json::{Value, json};

/// The menu id of a saved workspace: `clippy.workspace.load/<name>` loads it (names never hold a
/// slash).
pub const LOAD_PREFIX: &str = "clippy.workspace.load/";

/// PhotoCraft's Window › Workspace items that stay in Clippy mode; its workspaces and their
/// commands, which arrange PhotoCraft's panels, give way to Clippy's.
const KEPT: &[&str] = &["edit.keyboardShortcuts", "window.workspace.lockWorkspace"];

/// An open workspace dialog.
#[derive(Clone, Debug, PartialEq)]
enum Dialog {
    New { name: String, error: String },
    Delete { name: String, error: String },
}

fn dialog_id() -> egui::Id {
    super::area_id().with("workspace-dialog")
}

/// Window › Workspace in Clippy mode: the saved workspaces, New Workspace…, Delete Workspace…,
/// then PhotoCraft's Keyboard Shortcuts… and Lock Workspace.
pub(crate) fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    if !crate::mode::active(app) {
        return;
    }
    let path = vec!["Window".to_string(), "Workspace".to_string()];
    let Some(at) = items.iter().position(|i| i.path == path) else { return };
    let kept: Vec<MenuItem> = items.iter().filter(|i| i.path == path && KEPT.contains(&i.id.as_str())).cloned().collect();
    items.retain(|i| i.path != path);
    let item =
        |id: String, label: &str, enabled: bool| MenuItem { id, label: label.into(), path: path.clone(), shortcut: None, enabled, checked: None, color: None };
    let names = workspace_names(&app.session);
    let mut rows: Vec<MenuItem> = names.iter().map(|n| item(format!("{LOAD_PREFIX}{n}"), n, true)).collect();
    if !rows.is_empty() {
        rows.push(item("---".into(), "---", false));
    }
    rows.push(item(WORKSPACE_SAVE.into(), "New Workspace…", true));
    rows.push(item(WORKSPACE_DELETE.into(), "Delete Workspace…", !names.is_empty()));
    if !kept.is_empty() {
        rows.push(item("---".into(), "---", false));
        rows.extend(kept);
    }
    let at = at.min(items.len());
    items.splice(at..at, rows);
}

/// The workspace menu ids: a saved workspace loads; New and Delete without a name open their
/// dialog (with a name they run the engine command as it is).
pub(crate) fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if let Some(name) = id.strip_prefix(LOAD_PREFIX) {
        return Some(app.run(WORKSPACE_LOAD, json!({"name": name})));
    }
    let named = params.get("name").is_some();
    let dialog = match id {
        WORKSPACE_SAVE if !named => Dialog::New { name: String::new(), error: String::new() },
        WORKSPACE_DELETE if !named => Dialog::Delete { name: workspace_names(&app.session).into_iter().next().unwrap_or_default(), error: String::new() },
        _ => return None,
    };
    let kind = if matches!(dialog, Dialog::New { .. }) { "newWorkspace" } else { "deleteWorkspace" };
    ctx.data_mut(|d| d.insert_temp(dialog_id(), Some(dialog)));
    Some(Ok(json!({"dialog": kind})))
}

/// Draw the open workspace dialog.
pub(crate) fn dialogs(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(mut dialog) = ctx.data(|d| d.get_temp::<Option<Dialog>>(dialog_id())).flatten() else { return };
    let t = Tokens::get(ctx);
    let title = match dialog {
        Dialog::New { .. } => tl!("New Workspace"),
        Dialog::Delete { .. } => tl!("Delete Workspace"),
    };
    let mut answer = None;
    let names = workspace_names(&app.session);
    egui::Window::new(title)
        .id(dialog_id().with("window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(egui::Stroke::new(1.0, t.card_border)))
        .show(ctx, |ui| {
            ui.set_width(300.0);
            match &mut dialog {
                Dialog::New { name, error } => {
                    ui.label(egui::RichText::new(tl!("Name")).color(t.text_dim));
                    let field = widgets::text_field(ui, dialog_id().with("name"), name, tl!("Name"), 300.0, false);
                    if !field.has_focus() && name.is_empty() {
                        field.request_focus();
                    }
                    let exists = names.iter().any(|n| n.to_lowercase() == name.trim().to_lowercase());
                    if exists {
                        ui.label(egui::RichText::new(tl!("A workspace with this name exists.")).color(t.text_faint).small());
                    }
                    if !error.is_empty() {
                        ui.label(egui::RichText::new(error.as_str()).color(t.danger));
                    }
                    ui.add_space(8.0);
                    let ok = if exists { tl!("Replace") } else { tl!("OK") };
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    answer = widgets::dialog_buttons(
                        ui,
                        &[
                            DialogButton::new(ButtonRole::Default, ok, 80.0).enabled(!name.trim().is_empty()),
                            DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0),
                        ],
                    )
                    .or(enter.then_some(ButtonRole::Default));
                }
                Dialog::Delete { name, error } => {
                    for n in &names {
                        if ui.radio(name == n, n.as_str()).clicked() {
                            *name = n.clone();
                        }
                    }
                    if !error.is_empty() {
                        ui.label(egui::RichText::new(error.as_str()).color(t.danger));
                    }
                    ui.add_space(8.0);
                    answer = widgets::dialog_buttons(
                        ui,
                        &[
                            DialogButton::new(ButtonRole::Default, tl!("Delete"), 80.0).enabled(names.contains(name)),
                            DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0),
                        ],
                    );
                }
            }
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        answer = Some(ButtonRole::Cancel);
    }
    let next = match (answer, dialog) {
        (Some(ButtonRole::Default), Dialog::New { name, .. }) => match app.run(WORKSPACE_SAVE, json!({"name": name, "overwrite": true})) {
            Ok(_) => None,
            Err(error) => Some(Dialog::New { name, error }),
        },
        (Some(ButtonRole::Default), Dialog::Delete { name, .. }) => match app.run(WORKSPACE_DELETE, json!({"name": name})) {
            Ok(_) => None,
            Err(error) => Some(Dialog::Delete { name, error }),
        },
        (Some(ButtonRole::Cancel), _) => None,
        (_, dialog) => Some(dialog),
    };
    ctx.data_mut(|d| d.insert_temp(dialog_id(), next));
}
