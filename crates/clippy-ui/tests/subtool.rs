//! The sub tool data flow in the app: selection, painting with the imported brush, write-back,
//! Photoshop mode, reset, initial settings, unsupported sub tools and bad params.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use photocraft_clippy_sut::{Limits, Source as Pen, Sut, Value, known as k};
use photocraft_clippy_ui::subtool;
use photocraft_engine::paint::brush::Control;
use photocraft_ui_egui::state::Tool;
use photocraft_ui_egui::{PhotocraftApp, Services};
use serde_json::json;

fn app() -> PhotocraftApp {
    let mut services = Services::default();
    photocraft_clippy_ui::install(&mut services);
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.run("prefs.set", json!({"path": "interface.language", "value": "en"})).unwrap();
    app
}

fn write(sut: &Sut, dir: &std::path::Path, name: &str) -> String {
    let path = dir.join(name);
    sut.save(&path).unwrap();
    path.to_string_lossy().into_owned()
}

fn on_disk(path: &str) -> Sut {
    Sut::open(std::path::Path::new(path)).unwrap()
}

#[test]
fn selecting_a_gpen_sets_tool_brush_and_preference() {
    let dir = common::temp_dir("select");
    let path = write(&common::gpen(), &dir, "gpen.sut");
    let mut app = app();
    app.ui.tool = Tool::Move;
    let rev = app.session.tools.brush_rev;
    let out = app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    assert_eq!(out["name"], "G-Pen");
    assert_eq!(out["supported"], true);
    assert_eq!(out["tool"], "Brush");
    assert!(out["mapped"].as_array().unwrap().iter().any(|r| r == "sizeDynamics"));
    assert_eq!(app.ui.tool, Tool::Brush);
    assert!(app.session.tools.brush_rev > rev);
    let b = &app.session.tools.brush;
    assert_eq!(b.size, 12.5);
    assert_eq!(b.shape_dynamics.size.control, Control::PenPressure);
    assert_eq!(app.session.prefs().clippy.subtool, path);
    assert_eq!(app.run("clippy.subtool.get", json!({})).unwrap()["path"], path.as_str());
    // A frame right after changes nothing on disk.
    let bytes = std::fs::read(&path).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

/// Acceptance: painting with an imported G-pen uses its size and its pressure curve.
#[test]
fn painting_with_the_gpen_follows_its_size_and_pressure_curve() {
    let dir = common::temp_dir("paint");
    let path = write(&common::gpen(), &dir, "gpen.sut");
    let mut app = app();
    app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    // One horizontal line per pressure; its painted height is the dab diameter.
    let height = |app: &mut PhotocraftApp, y: f64, pressure: f64| -> usize {
        app.run("paint.stroke", json!({"points": [[20.0, y, pressure], [180.0, y, pressure]], "color": "#000000"})).unwrap();
        let doc = &app.session.active().unwrap().doc;
        let surface = doc.layer(app.session.active().unwrap().active_layer.unwrap()).unwrap().surface().unwrap();
        (y as i32 - 18..y as i32 + 18).filter(|&row| surface.rgba(100, row)[3] > 0.5).count()
    };
    let full = height(&mut app, 30.0, 1.0);
    let half = height(&mut app, 70.0, 0.4);
    // 12.5 px at full pressure; at 0.4 the curve gives 0.15: 18 % + 82 % × 0.15 ≈ 30 % ≈ 3.8 px.
    assert!((11..=14).contains(&full), "full pressure: {full} px");
    assert!((2..=6).contains(&half), "0.4 pressure: {half} px");
}

#[test]
fn changes_are_written_back_once_the_pointer_is_up() {
    let dir = common::temp_dir("writeback");
    let sut = common::sut("Rich", (10, 10), &common::rich_brush_values());
    let path = write(&sut, &dir, "rich.sut");
    let mut app = app();
    app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    let before = on_disk(&path).subtool.variant.unwrap();
    app.run("tools.setBrush", json!({"size": 30})).unwrap();
    subtool::tick(&mut app, true);
    assert_eq!(on_disk(&path).subtool.variant.unwrap().real(k::BRUSH_SIZE), Some(12.5), "not while the pointer is down");
    subtool::tick(&mut app, false);
    let after = on_disk(&path).subtool.variant.unwrap();
    assert_eq!(after.real(k::BRUSH_SIZE), Some(30.0));
    for cell in before.row.cells().iter().filter(|c| c.name != k::BRUSH_SIZE) {
        assert!(cell.value.same(after.get(&cell.name).unwrap()), "{} changed", cell.name);
    }
    // Through a command too (as MCP and the options bar do).
    app.run("tools.setBrush", json!({"opacity": 0.5})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(on_disk(&path).subtool.variant.unwrap().int(k::OPACITY), Some(50));
    // Another tool's brush is never written into the sub tool.
    app.ui.tool = Tool::Eraser;
    photocraft_ui_egui::paint_mouse::sync_tool_brush(&mut app);
    app.run("tools.setBrush", json!({"size": 99})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(on_disk(&path).subtool.variant.unwrap().real(k::BRUSH_SIZE), Some(30.0));
}

#[test]
fn photoshop_mode_never_touches_the_sut_and_switching_back_reloads_it() {
    let dir = common::temp_dir("photoshop");
    let path = write(&common::gpen(), &dir, "gpen.sut");
    let mut app = app();
    app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    app.run("clippy.mode.photoshop", json!({})).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    app.run("tools.setBrush", json!({"size": 50})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "Photoshop mode writes nothing");
    assert!(app.run("clippy.subtool.select", json!({"path": path})).is_err());
    assert!(app.run("clippy.subtool.writeBack", json!({})).is_err());
    // Changed elsewhere meanwhile: switching back reloads the file.
    let mut sut = on_disk(&path);
    sut.subtool.variant.as_mut().unwrap().set(k::BRUSH_SIZE, Value::Real(7.0));
    sut.save(std::path::Path::new(&path)).unwrap();
    app.run("clippy.mode.clippy", json!({})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(app.session.tools.brush.size, 7.0);
    assert_eq!(app.ui.tool, Tool::Brush);
}

#[test]
fn the_preference_loads_the_sub_tool_at_start() {
    let dir = common::temp_dir("start");
    let path = write(&common::gpen(), &dir, "gpen.sut");
    let mut app = app();
    app.run("prefs.set", json!({"path": "clippy.subtool", "value": path})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(app.session.tools.brush.size, 12.5);
    // A file that can't be read is reported once, not every frame.
    app.run("prefs.set", json!({"path": "clippy.subtool", "value": dir.join("missing.sut").to_string_lossy()})).unwrap();
    subtool::tick(&mut app, false);
    assert!(!app.ui.status.is_empty());
    app.ui.status.clear();
    subtool::tick(&mut app, false);
    assert!(app.ui.status.is_empty());
}

#[test]
fn reset_and_register_as_initial_settings() {
    let dir = common::temp_dir("reset");
    let path = write(&common::gpen(), &dir, "gpen.sut");
    let mut app = app();
    app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    app.run("tools.setBrush", json!({"size": 40})).unwrap();
    app.run("clippy.subtool.saveAsDefault", json!({})).unwrap();
    let sut = on_disk(&path);
    assert_eq!(sut.subtool.init_variant.as_ref().unwrap().real(k::BRUSH_SIZE), Some(40.0));
    assert_eq!(sut.subtool.init_variant.as_ref().unwrap().id(), Some(2), "the initial settings keep their id");
    app.run("tools.setBrush", json!({"size": 3})).unwrap();
    subtool::tick(&mut app, false);
    assert_eq!(on_disk(&path).subtool.variant.unwrap().real(k::BRUSH_SIZE), Some(3.0));
    app.run("clippy.subtool.reset", json!({})).unwrap();
    assert_eq!(app.session.tools.brush.size, 40.0);
    let sut = on_disk(&path);
    assert_eq!(sut.subtool.variant.as_ref().unwrap().real(k::BRUSH_SIZE), Some(40.0));
    assert_eq!(sut.subtool.variant.as_ref().unwrap().id(), Some(1));
    let e = sut.subtool.variant.unwrap().effector(k::BRUSH_SIZE_EFFECTOR).unwrap().unwrap();
    assert_eq!(e.minimum(Pen::Pressure), 18);
}

#[test]
fn unsupported_sub_tools_are_selected_with_a_notice() {
    let dir = common::temp_dir("unsupported");
    let path = write(&common::sut("Frame border", (60, 60), &[]), &dir, "frame.sut");
    let mut app = app();
    app.ui.tool = Tool::Lasso;
    let size = app.session.tools.brush.size;
    let out = app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    assert_eq!(out["supported"], false);
    assert!(out["notice"].as_str().unwrap().contains("60/60"));
    assert!(app.ui.status.contains("Frame border"));
    assert_eq!(app.ui.tool, Tool::Lasso);
    assert_eq!(app.session.tools.brush.size, size);
    assert_eq!(app.run("clippy.subtool.writeBack", json!({})).unwrap()["written"], json!([]));
}

#[test]
fn the_selection_pen_paints_in_quick_mask() {
    let dir = common::temp_dir("quickmask");
    let path = write(&common::sut("Selection pen", (10, 20), &[(k::BRUSH_SIZE, Value::Real(20.0))]), &dir, "selpen.sut");
    let mut app = app();
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.run("clippy.subtool.select", json!({"path": path})).unwrap();
    assert_eq!(app.ui.tool, Tool::Brush);
    assert!(app.session.active().unwrap().doc.quick_mask.is_some());
}

#[test]
fn export_writes_the_current_brush_as_a_new_sub_tool() {
    let dir = common::temp_dir("export");
    let mut app = app();
    app.ui.tool = Tool::Brush;
    app.run("tools.setBrush", json!({"size": 33})).unwrap();
    let path = dir.join("made.sut").to_string_lossy().into_owned();
    let out = app.run("clippy.subtool.export", json!({"path": path, "name": "Made"})).unwrap();
    assert_eq!(out["tool"], "Brush");
    let sut = Sut::from_bytes(&std::fs::read(&path).unwrap(), &Limits::default()).unwrap();
    assert_eq!(sut.subtool.name(), "Made");
    assert_eq!(sut.subtool.variant.unwrap().real(k::BRUSH_SIZE), Some(33.0));
    app.ui.tool = Tool::Crop;
    assert!(app.run("clippy.subtool.export", json!({"path": path})).is_err());
}

#[test]
fn bad_params_and_states_are_errors() {
    let dir = common::temp_dir("bad");
    let garbage = dir.join("garbage.sut");
    std::fs::write(&garbage, b"not a database").unwrap();
    let mut app = app();
    for id in ["clippy.subtool.writeBack", "clippy.subtool.reset", "clippy.subtool.saveAsDefault"] {
        assert!(app.run(id, json!({})).is_err(), "{id} without a sub tool");
    }
    for p in [
        json!({}),
        json!({"path": 1}),
        json!({"path": ""}),
        json!({"path": "a\u{0}b"}),
        json!({"path": "x".repeat(5000)}),
        json!({"path": garbage.to_string_lossy()}),
        json!({"path": dir.to_string_lossy()}),
        json!([1]),
        json!("x"),
    ] {
        assert!(app.run("clippy.subtool.select", p.clone()).is_err(), "{p}");
        assert_eq!(app.session.prefs().clippy.subtool, "", "{p}");
    }
    assert_eq!(app.run("clippy.subtool.get", json!({})).unwrap(), json!({"path": null}));
    for p in [json!({"path": dir.join("e.sut").to_string_lossy(), "name": 3}), json!({"path": dir.join("e.sut").to_string_lossy(), "name": ""})] {
        assert!(app.run("clippy.subtool.export", p).is_err());
    }
}

#[test]
fn photocraft_without_clippy_refuses_the_commands() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    assert!(app.run("clippy.subtool.select", json!({"path": "/nonexistent.sut"})).is_err());
    assert!(app.run("clippy.subtool.get", json!({})).is_err());
}

#[test]
fn the_extension_hides_brush_presets() {
    assert!(photocraft_clippy_ui::extension().hides_brush_presets);
}
