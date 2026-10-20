//! The palettes' name dialog: New Tool…, New Sub Tool Group…, New Sub Tool… and Rename…. It asks
//! for a name and runs the engine command; an error (a clash, a name that can't be a file name)
//! shows in the dialog, which stays open.

use photocraft_clippy_stock::EntryPath;
use photocraft_engine::clippy_cmds::{STOCK_GROUP_CREATE, STOCK_RENAME, STOCK_TOOL_CREATE, SUBTOOL_EXPORT};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::Tokens;
use photocraft_ui_egui::widgets::{self, ButtonRole, DialogButton};
use serde_json::{Value, json};

/// What the name is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Purpose {
    NewTool,
    NewGroup {
        tool: String,
    },
    /// A new sub tool from PhotoCraft's current tool and brush, in a group.
    NewSubTool {
        group: EntryPath,
    },
    Rename {
        path: EntryPath,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Prompt {
    purpose: Purpose,
    name: String,
    error: String,
}

fn dialog_id() -> egui::Id {
    egui::Id::new("clippy-palettes-name-dialog")
}

/// Open the name dialog for `purpose`, starting with `name`.
pub(crate) fn ask(ctx: &egui::Context, purpose: Purpose, name: &str) {
    ctx.data_mut(|d| d.insert_temp(dialog_id(), Some(Prompt { purpose, name: name.to_owned(), error: String::new() })));
}

/// Draw the open name dialog.
pub fn dialogs(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(mut prompt) = ctx.data(|d| d.get_temp::<Option<Prompt>>(dialog_id())).flatten() else { return };
    let t = Tokens::get(ctx);
    let title = match &prompt.purpose {
        Purpose::NewTool => tl!("New Tool"),
        Purpose::NewGroup { .. } => tl!("New Sub Tool Group"),
        Purpose::NewSubTool { .. } => tl!("New Sub Tool"),
        Purpose::Rename { path: EntryPath::Tool(_) } => tl!("Rename Tool"),
        Purpose::Rename { path: EntryPath::Group(..) } => tl!("Rename Sub Tool Group"),
        Purpose::Rename { .. } => tl!("Rename Sub Tool"),
    };
    let mut answer = None;
    egui::Window::new(title)
        .id(dialog_id().with("window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(egui::Stroke::new(1.0, t.card_border)))
        .show(ctx, |ui| {
            ui.set_width(300.0);
            ui.label(egui::RichText::new(tl!("Name")).color(t.text_dim));
            let field = widgets::text_field(ui, dialog_id().with("name"), &mut prompt.name, tl!("Name"), 300.0, false);
            if !field.has_focus() && ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            if !prompt.error.is_empty() {
                ui.label(egui::RichText::new(prompt.error.as_str()).color(t.danger));
            }
            ui.add_space(8.0);
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            answer = widgets::dialog_buttons(
                ui,
                &[
                    DialogButton::new(ButtonRole::Default, tl!("OK"), 80.0).enabled(!prompt.name.trim().is_empty()),
                    DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0),
                ],
            )
            .or(enter.then_some(ButtonRole::Default));
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        answer = Some(ButtonRole::Cancel);
    }
    let next = match answer {
        Some(ButtonRole::Default) if !prompt.name.trim().is_empty() => match run(app, &prompt.purpose, prompt.name.trim()) {
            Ok(_) => None,
            Err(error) => Some(Prompt { error, ..prompt }),
        },
        Some(ButtonRole::Cancel) => None,
        _ => Some(prompt),
    };
    ctx.data_mut(|d| d.insert_temp(dialog_id(), next));
}

/// Carry out `purpose` with `name`.
pub(crate) fn run(app: &mut PhotocraftApp, purpose: &Purpose, name: &str) -> Result<Value, String> {
    match purpose {
        Purpose::NewTool => app.run(STOCK_TOOL_CREATE, json!({"name": name})),
        Purpose::NewGroup { tool } => {
            let made = app.run(STOCK_GROUP_CREATE, json!({"tool": tool, "name": name}))?;
            // The new group is the one the tool shows.
            if let Some(EntryPath::Group(t, g)) = made.get("path").and_then(Value::as_str).and_then(|p| EntryPath::parse(p).ok()) {
                let v = super::view(app);
                v.shown_tool = Some(t.clone());
                v.groups.insert(t, g);
            }
            Ok(made)
        }
        Purpose::NewSubTool { group } => new_subtool(app, group, name),
        Purpose::Rename { path } => app.run(STOCK_RENAME, json!({"path": path.to_string(), "name": name})),
    }
}

/// A new sub tool from PhotoCraft's current tool and brush (`clippy.subtool.export`) in `group`,
/// then selected.
fn new_subtool(app: &mut PhotocraftApp, group: &EntryPath, name: &str) -> Result<Value, String> {
    photocraft_clippy_stock::names::validate(name).map_err(|e| e.to_string())?;
    let root = super::root(app).ok_or_else(|| tl!("The sub tool stock isn't open.").to_string())?;
    let EntryPath::Group(tool, g) = group else { return Err(tl!("Choose a sub tool group first.").into()) };
    let entry = EntryPath::SubTool(tool.clone(), g.clone(), name.to_owned());
    let disk = entry.to_path(&root);
    if disk.exists() {
        return Err(format!("{name}: {}", tl!("a sub tool of this name exists in the group.")));
    }
    // Clippy's own change: the stock reloads at once.
    #[cfg(not(target_arch = "wasm32"))]
    photocraft_clippy_stock::own::note(&disk);
    let made = app.run(SUBTOOL_EXPORT, json!({"path": disk.to_string_lossy(), "name": name}))?;
    super::select(app, &entry)?;
    Ok(made)
}
