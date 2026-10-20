//! The active sub tool and its data flow (feature M8). The `.sut` of the active sub tool is the
//! source of truth; PhotoCraft's tool, brush and tool options are its working copy:
//!
//! 1. `clippy.subtool.select` reads the `.sut`, selects its PhotoCraft tool (as the tool column
//!    does), sets the brush and the tool options it maps ([`crate::mapping`]) and notes the brush
//!    change. The path is the preference `clippy.subtool`, so a restart (or `prefs.set`) loads it.
//! 2. Every frame in Clippy mode, once the pointer is up, a changed brush (`brush_rev`) or tool
//!    option is written back with `clippy.subtool.writeBack`: only the columns of the changed
//!    settings, the rest untouched, saved atomically. So edits made through commands, MCP or the
//!    options bar all land in the sub tool.
//! 3. Photoshop mode never reads or writes a `.sut`; switching back reloads the active sub tool.
//! 4. `clippy.subtool.reset` restores the initial settings (`NodeInitVariantID`);
//!    `clippy.subtool.saveAsDefault` makes the current settings the initial ones.
//! 5. A locked sub tool (`clippy.subtool.lock`, the `.sut`'s `NodeLock`, E10) takes changes in the
//!    working copy but never writes them; selecting it again brings back what the file holds.
//!
//! A sub tool PhotoCraft has no tool for is selected with a notice and changes nothing.

use photocraft_clippy_sut::{Sut, Variant};
use photocraft_engine::BrushSettings;
use photocraft_engine::clippy_cmds::{
    ClippyMode, SUBTOOL_COMMANDS, SUBTOOL_EXPORT, SUBTOOL_GET, SUBTOOL_LOCK, SUBTOOL_RESET, SUBTOOL_SAVE_AS_DEFAULT, SUBTOOL_SELECT, SUBTOOL_WRITE_BACK,
};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::state::Tool;
use serde_json::{Value, json};

use crate::mapping::{self, Choice, Ctx, Extra, Scope, Source, Working, tool};

/// Longest path accepted in params.
const MAX_PATH_BYTES: usize = 4096;

/// The sub tool in use.
pub struct Active {
    /// Its `.sut`.
    pub path: String,
    /// The file as read, with the settings written back since.
    pub sut: Sut,
    pub choice: Choice,
    /// The working copy as last applied or written back: what the `.sut` holds.
    pub applied: Working,
    /// `ToolState::brush_rev` when `applied` was taken.
    pub brush_rev: u64,
    /// The conversion context of the selection (sizes in physical units keep converting at the
    /// resolution they were read at).
    pub ctx: Ctx,
}

/// Clippy's sub tool state in the app ([`PhotocraftApp::extension_data`]).
#[derive(Default)]
pub struct State {
    pub active: Option<Active>,
    /// Photoshop mode ran since the last Clippy frame: the active sub tool is reloaded.
    away: bool,
    /// A path that failed to load, so the frame doesn't try it again every frame.
    failed: Option<String>,
    /// The palettes' view of the sub tool stock (M9).
    pub(crate) view: crate::palettes::View,
}

pub(crate) fn state(app: &mut PhotocraftApp) -> &mut State {
    app.extension_data::<State>()
}

/// The active sub tool, if any.
pub fn active(app: &mut PhotocraftApp) -> Option<&Active> {
    state(app).active.as_ref()
}

/// Runs the `clippy.subtool.*` commands (the extension's command hook); `None` for other ids.
pub fn command(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !SUBTOOL_COMMANDS.contains(&id) {
        return None;
    }
    if !(params.is_object() || params.is_null()) {
        return Some(Err(format!("{id}: params must be a JSON object")));
    }
    if crate::mode::current(app.session.prefs()) != ClippyMode::Clippy {
        return Some(Err(tl!("Sub tools belong to the Clippy interface (Window › Interface › Clippy).").into()));
    }
    Some(match id {
        SUBTOOL_SELECT => path_param(id, params).and_then(|path| select(app, &path)),
        SUBTOOL_GET => Ok(summary(app)),
        SUBTOOL_WRITE_BACK => write_back(app),
        SUBTOOL_RESET => reset(app),
        SUBTOOL_SAVE_AS_DEFAULT => save_as_default(app),
        SUBTOOL_EXPORT => path_param(id, params).and_then(|path| export(app, &path, params.get("name"))),
        SUBTOOL_LOCK => match params.get("on") {
            Some(Value::Bool(on)) => lock(app, *on),
            _ => Err(format!("{id}: `on` must be true or false")),
        },
        _ => return None,
    })
}

fn path_param(id: &str, params: &Value) -> Result<String, String> {
    match params.get("path") {
        Some(Value::String(p)) if !p.is_empty() && p.len() <= MAX_PATH_BYTES && !p.contains('\0') => Ok(p.clone()),
        Some(Value::String(_)) => Err(format!("{id}: `path` must be a file path of 1 to {MAX_PATH_BYTES} bytes")),
        Some(_) => Err(format!("{id}: `path` must be a string")),
        None => Err(format!("{id}: missing `path` (the .sut file)")),
    }
}

/// Once per frame (the extension's frame hook).
pub fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let pointer_down = ctx.input(|i| i.pointer.any_down());
    tick(app, pointer_down);
}

/// The active sub tool's file moved to `path` (renamed, moved to another group or tool, in Clippy
/// or a file manager): the preference and the data flow follow it without reading it again. An
/// empty `path` means it is gone: nothing is selected, and nothing is written.
pub fn retarget(app: &mut PhotocraftApp, path: &str) {
    let st = state(app);
    match st.active.as_mut() {
        Some(a) if !path.is_empty() => a.path = path.to_owned(),
        _ => st.active = None,
    }
    st.failed = None;
    if app.session.prefs().clippy.subtool != path {
        let path = path.to_owned();
        app.session.edit_prefs(|p| p.clippy.subtool = path);
    }
}

/// The frame's part of the data flow: load the sub tool the preference names (at start, after a
/// `prefs.set`, back from Photoshop mode), and write changes back once the pointer is up.
pub fn tick(app: &mut PhotocraftApp, pointer_down: bool) {
    if crate::mode::current(app.session.prefs()) != ClippyMode::Clippy {
        state(app).away = true;
        return;
    }
    let want = app.session.prefs().clippy.subtool.clone();
    let st = state(app);
    let away = std::mem::take(&mut st.away);
    let loaded = st.active.as_ref().map(|a| a.path.clone());
    if want.is_empty() {
        st.active = None;
    } else if (away || loaded.as_deref() != Some(want.as_str())) && st.failed.as_deref() != Some(want.as_str()) {
        if let Err(e) = app.run(SUBTOOL_SELECT, json!({"path": want})) {
            // The preference names another sub tool now: the old one is no longer written to.
            let st = state(app);
            st.failed = Some(want);
            st.active = None;
            app.ui.status = e;
        }
        return;
    }
    if pointer_down || !changed(app) {
        return;
    }
    if let Err(e) = app.run(SUBTOOL_WRITE_BACK, json!({})) {
        app.ui.status = e;
    }
}

/// Whether the working copy moved since it was last applied or written back, while it is the
/// sub tool's (another tool's brush is never written into it).
fn changed(app: &mut PhotocraftApp) -> bool {
    let Some(tool) = state(app).active.as_ref().and_then(|a| a.choice.tool()) else { return false };
    if !owns_working_copy(app, tool) {
        return false;
    }
    let rev = app.session.tools.brush_rev;
    let (options, mode, snap) = (app.ui.tool_options.clone(), app.ui.selection_mode, app.ui.extras.snap);
    state(app).active.as_ref().is_some_and(|a| a.brush_rev != rev || a.applied.options != options || a.applied.selection_mode != mode || a.applied.snap != snap)
}

/// What the Tool Property palette shows of the active sub tool.
pub(crate) struct Info {
    pub name: String,
    /// Its PhotoCraft tool; `None` for a sub tool PhotoCraft can't map.
    pub tool: Option<Tool>,
    pub notice: Option<String>,
    pub locked: bool,
    /// The ids of the settings shown in Tool Property (`VariantShowParam`), in order.
    pub show: Vec<u32>,
    /// Whether PhotoCraft's tool and brush are this sub tool's now (another tool may have been
    /// chosen through a shortcut or PhotoCraft's tool column meanwhile).
    pub current: bool,
}

/// The active sub tool for the palettes, if any.
pub(crate) fn info(app: &mut PhotocraftApp) -> Option<Info> {
    let a = state(app).active.as_ref()?;
    let sub = &a.sut.subtool;
    let tool = a.choice.tool();
    let notice = match &a.choice {
        Choice::Unsupported { reason } => Some(reason.clone()),
        Choice::Tool { .. } => None,
    };
    let show = sub.variant.as_ref().and_then(|v| v.show_params()).and_then(Result::ok).map(|p| p.ids).unwrap_or_default();
    let (name, locked) = (sub.name().to_owned(), sub.lock());
    let current = tool.is_some_and(|t| owns_working_copy(app, t));
    Some(Info { name, tool, notice, locked, show, current })
}

/// The brush the active sub tool paints with now (the working copy), when it is a brush and
/// PhotoCraft's brush is its.
pub(crate) fn live_brush(app: &mut PhotocraftApp) -> Option<BrushSettings> {
    let tool = state(app).active.as_ref()?.choice.tool()?;
    (Scope::Brush.applies(tool) && owns_working_copy(app, tool)).then(|| app.session.tools.brush.clone())
}

/// Make the active sub tool's tool and brush PhotoCraft's again (after another tool was chosen).
pub(crate) fn reselect(app: &mut PhotocraftApp) -> Result<Value, String> {
    let path = state(app).active.as_ref().map(|a| a.path.clone()).ok_or_else(|| tl!("No sub tool is selected.").to_string())?;
    app.run(SUBTOOL_SELECT, json!({"path": path}))
}

/// PhotoCraft's working copy as it is now.
pub fn working(app: &PhotocraftApp) -> Working {
    Working { brush: app.session.tools.brush.clone(), options: app.ui.tool_options.clone(), selection_mode: app.ui.selection_mode, snap: app.ui.extras.snap }
}

/// The conversion context: the active document's resolution.
fn conversion(app: &PhotocraftApp) -> Ctx {
    let ppi = app.session.active().map(|st| f64::from(st.doc.resolution_dpi)).filter(|p| p.is_finite() && *p > 0.0).unwrap_or(72.0);
    Ctx { ppi }
}

/// Whether the shell's tool (and the brush it has switched in) is the sub tool's, so the working
/// copy is this sub tool's and not another tool's.
fn owns_working_copy(app: &PhotocraftApp, tool: Tool) -> bool {
    app.ui.tool == tool && (!photocraft_ui_egui::paint_mouse::has_brush_picker(tool) || app.ui.brush_tool == tool)
}

#[cfg(not(target_arch = "wasm32"))]
fn load(path: &str) -> Result<Sut, String> {
    Sut::open(std::path::Path::new(path)).map_err(|e| format!("{} {path}: {e}", tl!("The sub tool couldn't be read:")))
}

#[cfg(not(target_arch = "wasm32"))]
fn save(path: &str, sut: &Sut) -> Result<(), String> {
    sut.save(std::path::Path::new(path)).map_err(|e| format!("{} {path}: {e}", tl!("The sub tool couldn't be saved:")))
}

#[cfg(target_arch = "wasm32")]
fn load(_: &str) -> Result<Sut, String> {
    Err(tl!("Sub tool files can't be opened in the web version.").into())
}

#[cfg(target_arch = "wasm32")]
fn save(_: &str, _: &Sut) -> Result<(), String> {
    Err(tl!("Sub tool files can't be saved in the web version.").into())
}

/// `clippy.subtool.select`: read the file and make it the active sub tool.
fn select(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let sut = load(path)?;
    activate(app, path, sut)?;
    if app.session.prefs().clippy.subtool != path {
        let path = path.to_owned();
        app.session.edit_prefs(|p| p.clippy.subtool = path);
    }
    state(app).failed = None;
    Ok(summary(app))
}

/// Apply `sut` to PhotoCraft and make it the active sub tool.
fn activate(app: &mut PhotocraftApp, path: &str, sut: Sut) -> Result<(), String> {
    let choice = tool::choose_subtool(&sut.subtool);
    let ctx = conversion(app);
    match &choice {
        Choice::Tool { tool, extra } => {
            let variant = sut.subtool.variant.as_ref().ok_or_else(|| tl!("The sub tool has no settings (no NodeVariantID).").to_string())?;
            apply(app, *tool, *extra, &Source { variant, materials: &sut.subtool.materials, ctx });
            if *tool == Tool::Eraser && variant.flag(photocraft_clippy_sut::known::BRUSH_ERASE_ALL_LAYER) == Some(true) {
                app.ui.status =
                    format!("{}: {}", sut.subtool.name(), tl!("PhotoCraft erases on the current layer only; erasing all layers stays in the sub tool."));
            }
        }
        Choice::Unsupported { reason } => {
            app.ui.status = format!("{}: {reason}. {}", sut.subtool.name(), tl!("The sub tool is selected; PhotoCraft's tool stays as it is."));
        }
    }
    let applied = working(app);
    let brush_rev = app.session.tools.brush_rev;
    state(app).active = Some(Active { path: path.to_owned(), sut, choice, applied, brush_rev, ctx });
    Ok(())
}

/// Set PhotoCraft's tool, brush and tool options from a sub tool's settings.
fn apply(app: &mut PhotocraftApp, tool: Tool, extra: Extra, src: &Source<'_>) {
    let base = Working { brush: BrushSettings::default(), ..working(app) };
    let mut w = mapping::read(src, tool, base);
    match extra {
        Extra::FixedSpacing => w.brush.spacing_enabled = true,
        Extra::AutoSelectLayer => {
            w.options.move_auto_select = true;
            w.options.move_target = "layer".into();
        }
        Extra::ShowTransform => w.options.move_show_transform = true,
        Extra::QuickMask | Extra::None => {}
    }
    photocraft_ui_egui::extension::select_tool(app, tool);
    if Scope::Brush.applies(tool) {
        // The colours, the erase flag and the stroke seed are the session's, not the tool's.
        let cur = &app.session.tools.brush;
        let brush = BrushSettings { color: cur.color, background: cur.background, erase: cur.erase, seed: cur.seed, ..w.brush };
        app.session.tools.brush = brush;
        app.session.tools.current_preset = None;
        app.session.tools.brush_changed();
    }
    app.ui.tool_options = w.options;
    app.ui.selection_mode = w.selection_mode;
    app.ui.extras.snap = w.snap;
    // The selection pen paints the selection: Quick Mask, when there is a document to mask.
    if extra == Extra::QuickMask
        && app.session.active().is_some_and(|st| st.doc.quick_mask.is_none())
        && let Err(e) = app.run("select.editInQuickMaskMode", json!({"on": true}))
    {
        app.ui.status = e;
    }
}

/// Take the active sub tool out of the state for `f`, and put it back.
fn with_active<T>(app: &mut PhotocraftApp, f: impl FnOnce(&mut PhotocraftApp, &mut Active) -> Result<T, String>) -> Result<T, String> {
    let mut active = state(app).active.take().ok_or_else(|| tl!("No sub tool is selected.").to_string())?;
    let result = f(app, &mut active);
    state(app).active = Some(active);
    result
}

/// `clippy.subtool.writeBack`: write the changed settings into the `.sut`.
fn write_back(app: &mut PhotocraftApp) -> Result<Value, String> {
    with_active(app, |app, a| {
        let Some(tool) = a.choice.tool() else { return Ok(json!({"written": [], "reason": "unsupported sub tool"})) };
        if !owns_working_copy(app, tool) {
            return Ok(json!({"written": [], "reason": "another tool is in use"}));
        }
        let now = working(app);
        if a.sut.subtool.lock() {
            // Taken as seen, so the frame doesn't ask again until the next change.
            a.applied = now;
            a.brush_rev = app.session.tools.brush_rev;
            return Ok(json!({"written": [], "reason": "locked"}));
        }
        let variant = a.sut.subtool.variant.as_mut().ok_or_else(|| tl!("The sub tool has no settings (no NodeVariantID).").to_string())?;
        let written = mapping::write_changed(tool, &a.applied, &now, variant, a.ctx);
        // Taken as written even if the save fails, so a full disk doesn't retry every frame; the
        // next change saves again.
        a.applied = now;
        a.brush_rev = app.session.tools.brush_rev;
        if !written.is_empty() {
            save(&a.path, &a.sut)?;
        }
        Ok(json!({"written": written}))
    })
}

/// `clippy.subtool.lock`: set the active sub tool's `NodeLock` and save it. Unlocking brings back
/// the settings the file holds, so the working copy and the file agree again.
fn lock(app: &mut PhotocraftApp, on: bool) -> Result<Value, String> {
    let (path, sut) = with_active(app, |app, a| {
        if a.sut.subtool.lock() != on {
            if !a.sut.subtool.set_lock(on) {
                return Err(tl!("This sub tool file can't be locked (it has no NodeLock column).").into());
            }
            save(&a.path, &a.sut)?;
        }
        // What the working copy holds now counts as seen.
        a.applied = working(app);
        a.brush_rev = app.session.tools.brush_rev;
        Ok((a.path.clone(), a.sut.clone()))
    })?;
    if !on {
        activate(app, &path, sut)?;
    }
    Ok(json!({"path": path, "locked": on}))
}

/// Copy every column of `from` but its id into `to`.
fn copy_settings(from: &Variant, to: &mut Variant) {
    for cell in from.row.cells() {
        if !cell.name.eq_ignore_ascii_case("VariantID") {
            to.set(&cell.name, cell.value.clone());
        }
    }
}

/// `clippy.subtool.reset`: the initial settings become the current ones, saved and applied.
fn reset(app: &mut PhotocraftApp) -> Result<Value, String> {
    let (path, sut) = with_active(app, |_, a| {
        let init = a.sut.subtool.init_variant.clone().ok_or_else(|| tl!("The sub tool has no initial settings (no NodeInitVariantID).").to_string())?;
        let variant = a.sut.subtool.variant.as_mut().ok_or_else(|| tl!("The sub tool has no settings (no NodeVariantID).").to_string())?;
        copy_settings(&init, variant);
        save(&a.path, &a.sut)?;
        Ok((a.path.clone(), a.sut.clone()))
    })?;
    activate(app, &path, sut)?;
    Ok(summary(app))
}

/// `clippy.subtool.saveAsDefault`: the current settings (changes written back first) become the
/// initial ones.
fn save_as_default(app: &mut PhotocraftApp) -> Result<Value, String> {
    write_back(app)?;
    with_active(app, |_, a| {
        let current = a.sut.subtool.variant.clone().ok_or_else(|| tl!("The sub tool has no settings (no NodeVariantID).").to_string())?;
        let init = a.sut.subtool.init_variant.as_mut().ok_or_else(|| tl!("The sub tool has no initial settings (no NodeInitVariantID).").to_string())?;
        copy_settings(&current, init);
        save(&a.path, &a.sut)?;
        Ok(json!({"path": a.path}))
    })
}

/// `clippy.subtool.export`: the current tool and working copy as a new `.sut`.
fn export(app: &mut PhotocraftApp, path: &str, name: Option<&Value>) -> Result<Value, String> {
    let tool = app.ui.tool;
    let name = match name {
        None | Some(Value::Null) => format!("{tool:?}"),
        Some(Value::String(n)) if !n.trim().is_empty() && n.len() <= 256 => n.clone(),
        Some(_) => return Err(format!("{SUBTOOL_EXPORT}: `name` must be a name of 1 to 256 bytes")),
    };
    let seed = seed();
    let sut = mapping::reverse::new_sut(&name, tool, &working(app), conversion(app), seed)?;
    save(path, &sut)?;
    Ok(json!({"path": path, "name": name, "tool": tool}))
}

#[cfg(not(target_arch = "wasm32"))]
fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

#[cfg(target_arch = "wasm32")]
fn seed() -> u64 {
    0
}

/// `clippy.subtool.get`: the active sub tool and its mapping.
pub fn summary(app: &mut PhotocraftApp) -> Value {
    let Some(a) = active(app) else { return json!({"path": null}) };
    let sub = &a.sut.subtool;
    let tool = a.choice.tool();
    let (mapped, kept) = match &sub.variant {
        Some(v) => (tool.map(|t| mapping::mapped_rules(t, v)).unwrap_or_default(), mapping::kept_columns(tool, v)),
        None => (Vec::new(), Vec::new()),
    };
    let mut out = json!({
        "path": a.path,
        "name": sub.name(),
        "inputOp": sub.input_op(),
        "outputOp": sub.output_op(),
        "tool": tool,
        "supported": tool.is_some(),
        "locked": sub.lock(),
        "mapped": mapped,
        "kept": kept,
    });
    if let Choice::Unsupported { reason } = &a.choice {
        out["notice"] = json!(reason);
    }
    out
}
