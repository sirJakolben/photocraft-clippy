//! Clippy's tool palettes (M9) on a synthetic stock: what the Tool, Sub Tool and Tool Property
//! palettes show, choosing tools, groups and sub tools with the pointer, dragging a sub tool into
//! another group, a setting written back into the `.sut`, the lock, Duplicate, a move in a file
//! manager, and Edit › Brushes in both interfaces. No Clip Studio Paint data: every `.sut` is
//! generated.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use photocraft_clippy_sut::{ShowParams, Sut, Value, known as k};
use photocraft_clippy_ui::palettes;
use photocraft_ui_egui::state::Tool;
use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

/// A sub tool with Tool Property ids `show` and a node UUID of its own (`n`), as every Clip Studio
/// Paint sub tool has: a sub tool that moves is found again by it.
fn with_show(mut sut: Sut, n: u8, show: &[u32]) -> Sut {
    sut.subtool.node.set("NodeUuid", Value::Blob(vec![n; 16]));
    for v in [sut.subtool.variant.as_mut().unwrap(), sut.subtool.init_variant.as_mut().unwrap()] {
        v.set(k::SHOW_PARAM, Value::Blob(ShowParams { ids: show.to_vec() }.encode()));
    }
    sut
}

/// Pen (Glatt: G-Pen, Fineliner; Rau: Bleistift), Radierer (Hart: Hart) and Lineal (Gerade:
/// Lineal, which PhotoCraft has no tool for).
fn stock(name: &str) -> common::TempDir {
    let dir = common::temp_dir(&format!("palettes-{name}"));
    let put = |path: &str, sut: Sut| {
        let p = dir.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        sut.save(&p).unwrap();
    };
    put("Pen/Glatt/G-Pen.sut", with_show(common::gpen(), 1, &[1000, 10, 20, 40, 41, 42, 1241]));
    let mut fine = common::gpen();
    fine.subtool.set_name("Fineliner");
    put("Pen/Glatt/Fineliner.sut", with_show(fine, 2, &[1000, 10, 30, 20]));
    put("Pen/Rau/Bleistift.sut", with_show(common::sut("Bleistift", (11, 11), &[(k::BRUSH_SIZE, Value::Real(4.0))]), 3, &[1000]));
    put("Radierer/Hart/Hart.sut", with_show(common::sut("Hart", (12, 12), &[(k::BRUSH_SIZE, Value::Real(30.0))]), 4, &[1000, 20, 1030]));
    put("Lineal/Gerade/Lineal.sut", with_show(common::sut("Lineal", (91, 91), &[]), 5, &[]));
    dir
}

/// Clippy in English with its stock in `dir`, opened.
fn harness(dir: &Path) -> Harness {
    let dir = dir.to_string_lossy().into_owned();
    let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut services = Services::default();
        photocraft_clippy_ui::install(&mut services);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.run("prefs.set", json!({"path": "interface.language", "value": "en"})).unwrap();
        app.run("prefs.set", json!({"path": "clippy.stockDir", "value": dir})).unwrap();
        app
    });
    wait(&mut h, "the stock opens", |h| palettes::stock(h.state_mut()).is_some());
    h.run_steps(4);
    h
}

/// Step until `done` (the stock opens and changes on disk show in the background).
fn wait(h: &mut Harness, what: &str, mut done: impl FnMut(&mut Harness) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(h) {
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
        h.step();
    }
}

fn select(h: &mut Harness, dir: &Path, path: &str) {
    let disk = dir.join(format!("{path}.sut"));
    h.state_mut().run("clippy.subtool.select", json!({"path": disk})).unwrap();
    // The pointer off the palettes, so no tooltip doubles a label.
    h.event(egui::Event::PointerMoved(egui::pos2(SIZE.x / 2.0, SIZE.y / 2.0)));
    h.run_steps(6);
}

fn active(h: &mut Harness) -> Option<String> {
    palettes::active_entry(h.state_mut()).map(|p| p.to_string())
}

fn shown(h: &Harness, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

fn center(h: &Harness, label: &str) -> egui::Pos2 {
    h.query_all_by_label(label).last().unwrap_or_else(|| panic!("no `{label}`")).rect().center()
}

fn click_at(h: &mut Harness, pos: egui::Pos2, button: egui::PointerButton) {
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
}

fn click(h: &mut Harness, label: &str) {
    let pos = center(h, label);
    click_at(h, pos, egui::PointerButton::Primary);
}

/// Press at `from`, move to `to` in steps, release: a real pointer drag.
fn drag(h: &mut Harness, from: egui::Pos2, to: egui::Pos2) {
    drag_with(h, from, to, egui::Modifiers::NONE);
}

/// [`drag`] with `modifiers` held throughout.
fn drag_with(h: &mut Harness, from: egui::Pos2, to: egui::Pos2, modifiers: egui::Modifiers) {
    h.event(egui::Event::ModifiersChanged(modifiers));
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
    h.event(egui::Event::PointerMoved(from));
    h.step();
    h.event(button(from, true));
    h.step();
    for i in 1..=12 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 12.0)));
        h.step();
    }
    h.event(button(to, false));
    h.run_steps(4);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.step();
}

fn file(dir: &Path, path: &str) -> PathBuf {
    dir.join(format!("{path}.sut"))
}

#[test]
fn the_palettes_show_the_stock_and_the_active_sub_tool() {
    let dir = stock("show");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    // Tool: one icon per tool, the colours under them.
    for tool in ["Pen", "Radierer", "Lineal", "Main color", "Sub color", "Transparent color"] {
        assert!(shown(&h, tool), "Tool palette: {tool}");
    }
    // Sub Tool: the Pen's groups and the shown group's sub tools.
    for label in ["Glatt", "Rau", "G-Pen", "Fineliner", "⊕ Add Sub Tool"] {
        assert!(shown(&h, label), "Sub Tool palette: {label}");
    }
    assert!(!shown(&h, "Bleistift"), "only the shown group's tiles");
    // Tool Property: the G-Pen's settings with the eye on that PhotoCraft has (not the vector
    // magnet, 1241, nor the unnamed 42).
    for label in ["Brush Size", "Opacity", "Anti-aliasing", "Stabilization", "Adjust by speed"] {
        assert!(shown(&h, label), "Tool Property: {label}");
    }
    assert!(!shown(&h, "Blending mode"));
}

#[test]
fn clicking_a_tool_a_group_and_a_tile_selects_sub_tools() {
    let dir = stock("click");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    click(&mut h, "Fineliner");
    assert_eq!(active(&mut h).as_deref(), Some("Pen/Glatt/Fineliner"));
    assert_eq!(h.state().session.prefs().clippy.subtool, file(&dir.0, "Pen/Glatt/Fineliner").to_string_lossy());
    // A group tab selects its first sub tool; the tool remembers it.
    click(&mut h, "Rau");
    assert_eq!(active(&mut h).as_deref(), Some("Pen/Rau/Bleistift"));
    click(&mut h, "Radierer");
    assert_eq!(active(&mut h).as_deref(), Some("Radierer/Hart/Hart"));
    assert_eq!(h.state().ui.tool, Tool::Eraser);
    // Back to the pen: the sub tool it used last.
    click(&mut h, "Pen");
    assert_eq!(active(&mut h).as_deref(), Some("Pen/Rau/Bleistift"));
    // A sub tool PhotoCraft has no tool for is selected and says why.
    click(&mut h, "Lineal");
    assert_eq!(active(&mut h).as_deref(), Some("Lineal/Gerade/Lineal"));
    assert!(h.query_by_label_contains("no settings PhotoCraft can use").is_some());
}

#[test]
fn a_tile_dragged_onto_a_group_tab_moves_there_and_stays_selected() {
    let dir = stock("drag");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    let (from, to) = (center(&h, "G-Pen"), center(&h, "Rau"));
    drag(&mut h, from, to);
    wait(&mut h, "the move shows", |h| active(h).as_deref() == Some("Pen/Rau/G-Pen"));
    assert!(file(&dir.0, "Pen/Rau/G-Pen").is_file());
    assert!(!file(&dir.0, "Pen/Glatt/G-Pen").exists());
    // The data flow writes into the moved file, never the old place.
    assert_eq!(h.state().session.prefs().clippy.subtool, file(&dir.0, "Pen/Rau/G-Pen").to_string_lossy());
    h.state_mut().session.tools.brush.size = 40.0;
    h.state_mut().session.tools.brush_changed();
    h.run_steps(3);
    assert!(!file(&dir.0, "Pen/Glatt/G-Pen").exists());
    let sut = Sut::open(&file(&dir.0, "Pen/Rau/G-Pen")).unwrap();
    assert_eq!(sut.subtool.variant.as_ref().unwrap().row.get(k::BRUSH_SIZE), Some(&Value::Real(40.0)));
}

#[test]
fn a_tile_dropped_on_a_tool_icon_lands_in_its_group_and_ctrl_copies() {
    let dir = stock("tool-drop");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/Fineliner");
    let (from, to) = (center(&h, "Fineliner"), center(&h, "Radierer"));
    drag_with(&mut h, from, to, egui::Modifiers::COMMAND);
    wait(&mut h, "the copy is selected", |h| active(h).as_deref() == Some("Radierer/Hart/Fineliner"));
    assert!(file(&dir.0, "Pen/Glatt/Fineliner").is_file(), "Ctrl copies");
    let (a, b) = (Sut::open(&file(&dir.0, "Pen/Glatt/Fineliner")).unwrap(), Sut::open(&file(&dir.0, "Radierer/Hart/Fineliner")).unwrap());
    assert_ne!(a.subtool.uuid(), b.subtool.uuid(), "an independent copy");
    // Without Ctrl it moves: the pen's Bleistift into the eraser's group.
    select(&mut h, &dir.0, "Pen/Rau/Bleistift");
    let (from, to) = (center(&h, "Bleistift"), center(&h, "Radierer"));
    drag(&mut h, from, to);
    wait(&mut h, "the move shows", |h| active(h).as_deref() == Some("Radierer/Hart/Bleistift"));
    assert!(!file(&dir.0, "Pen/Rau/Bleistift").exists());
}

#[test]
fn a_setting_in_tool_property_is_written_into_the_sub_tool_unless_it_is_locked() {
    let dir = stock("property");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    let path = file(&dir.0, "Pen/Glatt/G-Pen");
    let by_speed = |p: &Path| Sut::open(p).unwrap().subtool.variant.unwrap().row.get(k::FLICKER_REDUCTION_BY_SPEED).cloned();
    let before = by_speed(&path);
    click(&mut h, "Adjust by speed");
    assert!(h.state().session.tools.brush.smoothing.by_speed.is_some());
    assert_ne!(by_speed(&path), before, "written once the pointer is up");
    // Locked: the file keeps what it holds.
    click(&mut h, "Lock");
    assert!(Sut::open(&path).unwrap().subtool.lock());
    let bytes = std::fs::read(&path).unwrap();
    click(&mut h, "Adjust by speed");
    assert!(h.state().session.tools.brush.smoothing.by_speed.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Unlocked: the settings come back from the file.
    click(&mut h, "Lock");
    assert!(!Sut::open(&path).unwrap().subtool.lock());
    assert!(h.state().session.tools.brush.smoothing.by_speed.is_some());
}

#[test]
fn duplicate_from_the_context_menu_selects_an_independent_copy() {
    let dir = stock("duplicate");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    let tile = center(&h, "G-Pen");
    click_at(&mut h, tile, egui::PointerButton::Secondary);
    click(&mut h, "Duplicate Sub Tool");
    wait(&mut h, "the copy is selected", |h| active(h).as_deref() == Some("Pen/Glatt/G-Pen 2"));
    let (a, b) = (Sut::open(&file(&dir.0, "Pen/Glatt/G-Pen")).unwrap(), Sut::open(&file(&dir.0, "Pen/Glatt/G-Pen 2")).unwrap());
    assert_ne!(a.subtool.uuid(), b.subtool.uuid());
    wait(&mut h, "the tile shows", |h| shown(h, "G-Pen 2"));
}

#[test]
fn a_rename_in_a_file_manager_takes_the_selection_along() {
    let dir = stock("rename");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Glatt/G-Pen");
    std::fs::rename(file(&dir.0, "Pen/Glatt/G-Pen"), file(&dir.0, "Pen/Glatt/Mein Stift")).unwrap();
    wait(&mut h, "the rename shows", |h| active(h).as_deref() == Some("Pen/Glatt/Mein Stift"));
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().clippy.subtool, file(&dir.0, "Pen/Glatt/Mein Stift").to_string_lossy());
    assert!(shown(&h, "Mein Stift"));
}

#[test]
fn edit_brushes_replaces_toolbar_in_clippy_mode_only() {
    let dir = stock("menu");
    let mut h = harness(&dir.0);
    let items = menus::menu_items(h.state());
    assert!(!items.iter().any(|i| i.id == "edit.toolbar"));
    for id in ["clippy.brushes.import", "clippy.brushes.export", "clippy.brushes.saveAs"] {
        let item = items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(item.path, ["Edit", "Brushes"]);
        assert!(item.enabled);
    }
    h.state_mut().run("clippy.mode.set", json!({"mode": "photoshop"})).unwrap();
    h.run_steps(2);
    let items = menus::menu_items(h.state());
    assert!(items.iter().any(|i| i.id == "edit.toolbar"));
    assert!(!items.iter().any(|i| i.id.starts_with("clippy.brushes.")));
}

#[test]
fn import_brushes_puts_a_sut_into_the_shown_group() {
    let dir = stock("import");
    let mut h = harness(&dir.0);
    select(&mut h, &dir.0, "Pen/Rau/Bleistift");
    let outside = common::temp_dir("palettes-import-src");
    let src = outside.0.join("Kohle.sut");
    common::sut("Kohle", (10, 10), &[(k::BRUSH_SIZE, Value::Real(20.0))]).save(&src).unwrap();
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "clippy.brushes.import", json!({"file": src})).unwrap();
    assert!(file(&dir.0, "Pen/Rau/Kohle").is_file());
    wait(&mut h, "the tile shows", |h| shown(h, "Kohle"));
    // Without a file and without a dialog service the import is cancelled, not an error.
    assert_eq!(menus::invoke(h.state_mut(), &ctx, "clippy.brushes.import", json!({})).unwrap_err(), "cancelled");
}
