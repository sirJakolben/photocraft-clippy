//! The mapping on synthetic sub tools: tools, brush fields, and round trips that keep every
//! mapped value. With `CLIPPY_CSP_FIXTURES` set, also on the real exported sub tools there
//! (`Sut-Exporte/*.sut`, never committed).
#![cfg(not(target_arch = "wasm32"))]

mod common;

use photocraft_clippy_sut::{Limits, Source as Pen, Sut, Value, Variant, known as k, synth};
use photocraft_clippy_ui::mapping::{self, Choice, Ctx, Source, Working, reverse, tool};
use photocraft_color::BlendMode;
use photocraft_engine::paint::brush::{Control, MaskMode, SpeedSmoothing, TipOrder};
use photocraft_ui_egui::state::Tool;

fn read(sut: &Sut, tool: Tool) -> Working {
    let v = sut.subtool.variant.as_ref().unwrap();
    mapping::read(&Source { variant: v, materials: &sut.subtool.materials, ctx: Ctx::default() }, tool, Working::default())
}

/// Every column of `a` reads the same in `b`.
fn assert_same_columns(a: &Variant, b: &Variant) {
    assert_eq!(a.row.len(), b.row.len());
    for cell in a.row.cells() {
        let other = b.get(&cell.name).unwrap_or_else(|| panic!("{} missing", cell.name));
        assert!(cell.value.same(other), "{}: {:?} became {:?}", cell.name, cell.value, other);
    }
}

#[test]
fn a_gpen_reads_as_a_pressure_brush() {
    let sut = common::gpen();
    assert_eq!(tool::choose_subtool(&sut.subtool), Choice::Tool { tool: Tool::Brush, extra: mapping::Extra::None });
    let b = read(&sut, Tool::Brush).brush;
    assert_eq!(b.size, 12.5);
    assert!(b.shape_dynamics.enabled);
    assert_eq!(b.shape_dynamics.size.control, Control::PenPressure);
    assert!((b.shape_dynamics.size.minimum - 0.18).abs() < 1e-6);
    assert!((b.shape_dynamics.size.curve.eval(0.4) - 0.15).abs() < 1e-4);
    assert!(!b.pressure_size, "pressure comes from the pen settings only");
    assert_eq!(b.opacity, 1.0);
    assert_eq!(b.aa_level, Some(2));
    assert!((b.spacing - 0.08).abs() < 1e-6);
    assert!((b.smoothing.amount - 0.12).abs() < 1e-6);
}

#[test]
fn every_rich_brush_column_reads_into_its_field() {
    let sut = common::sut("Rich", (10, 10), &common::rich_brush_values());
    let w = read(&sut, Tool::Brush);
    let b = &w.brush;
    assert!(!b.min_one_px && b.size_follows_view);
    assert!((b.opacity - 0.87).abs() < 1e-6);
    assert!(b.transfer.enabled && b.transfer.opacity.control == Control::PenTilt);
    assert_eq!(b.mode, BlendMode::ColorDodge, "Glow Dodge reads as Color Dodge");
    assert!(b.color_dynamics.enabled && !b.color_dynamics.per_tip);
    assert!((b.color_dynamics.hue_jitter - 30.0 / 180.0).abs() < 1e-6);
    assert_eq!(b.aa_level, Some(3));
    assert_eq!(b.tip_order, TipOrder::RoundTrip);
    assert!((b.hardness - 0.7).abs() < 1e-6 && (b.roundness - 0.65).abs() < 1e-6);
    assert!((b.shape_dynamics.roundness.jitter - 0.5).abs() < 1e-6);
    assert_eq!(b.angle, 120.0, "vertical thickness turns the tip");
    assert_eq!(b.shape_dynamics.angle.control, Control::PenTilt);
    assert!((b.shape_dynamics.angle.jitter - 0.25).abs() < 1e-6);
    assert!(b.flip_y && !b.flip_x);
    assert!((b.flow - 0.9).abs() < 1e-6 && b.transfer.flow.control == Control::Velocity);
    assert!(b.scattering.enabled && (b.scattering.scatter.jitter - 2.0).abs() < 1e-6 && b.scattering.count == 8);
    assert_eq!(b.shape_dynamics.size.controls.as_slice()[0].control, Control::Velocity);
    assert!(b.blend_darken && !b.build_up);
    let t = &b.texture;
    assert!(!t.enabled, "no texture image in the file");
    assert!((t.depth - 0.6).abs() < 1e-6 && t.invert && (t.scale - 1.5).abs() < 1e-6 && t.rotation == 15.0 && t.each_tip);
    assert!((t.brightness + 0.2).abs() < 1e-6 && (t.contrast - 0.1).abs() < 1e-6 && t.mode == MaskMode::Overlay);
    assert!(b.dual_brush.enabled && (b.dual_brush.size - 6.25).abs() < 1e-6 && b.dual_brush.mode == MaskMode::Darken);
    assert!(b.wet_edges && (b.wet_edge.width - 0.48).abs() < 1e-6 && (b.wet_edge.interior - 0.6).abs() < 1e-6);
    assert_eq!(b.smoothing.by_speed, Some(SpeedSmoothing::ReduceFast));
    assert_eq!(b.smoothing.stickiness, 3);
    assert!(!w.snap);
    assert!(b.taper.start == 40.0 && b.taper.end == 25.0 && b.taper.by_speed);
}

/// Acceptance: .sut → PhotoCraft → .sut changes no mapped value, even when every rule writes.
#[test]
fn a_round_trip_keeps_every_mapped_value() {
    for (tool, ops) in [(Tool::Brush, (10, 10)), (Tool::Eraser, (12, 12)), (Tool::Blur, (10, 10)), (Tool::MixerBrush, (10, 10))] {
        let mut values = common::rich_brush_values();
        values.push((k::BRUSH_BLUR, Value::Integer(35)));
        let sut = common::sut("Weichzeichnen", ops, &values);
        let before = sut.subtool.variant.clone().unwrap();
        let w = read(&sut, tool);
        let mut after = before.clone();
        mapping::write_all(tool, &w, &mut after, Ctx::default());
        assert_same_columns(&before, &after);
        // And through the file.
        let mut written = sut.clone();
        written.subtool.variant = Some(after);
        let back = Sut::from_bytes(&written.to_bytes().unwrap(), &Limits::default()).unwrap();
        assert_same_columns(&before, back.subtool.variant.as_ref().unwrap());
        assert_eq!(read(&back, tool), w);
    }
}

#[test]
fn only_changed_settings_are_written() {
    let sut = common::sut("Rich", (10, 10), &common::rich_brush_values());
    let before = sut.subtool.variant.clone().unwrap();
    let w = read(&sut, Tool::Brush);
    let mut now = w.clone();
    // As tools.setBrush resizes: the dual tip keeps its share of the size.
    now.brush.resize_with_dual_brush(30.0);
    now.brush.shape_dynamics.size.minimum = 0.5;
    let mut after = before.clone();
    let written = mapping::write_changed(Tool::Brush, &w, &now, &mut after, Ctx::default());
    assert_eq!(written, ["size", "sizeDynamics"]);
    assert_eq!(after.real(k::BRUSH_SIZE), Some(30.0));
    let e = after.effector(k::BRUSH_SIZE_EFFECTOR).unwrap().unwrap();
    assert_eq!(e.minimum(Pen::Pressure), 50);
    assert!(e.active().contains(Pen::Velocity), "the other pen settings stay");
    for cell in before.row.cells().iter().filter(|c| c.name != k::BRUSH_SIZE && c.name != k::BRUSH_SIZE_EFFECTOR) {
        assert!(cell.value.same(after.get(&cell.name).unwrap()), "{}", cell.name);
    }
    // Nothing changed: nothing written.
    let mut same = before.clone();
    assert!(mapping::write_changed(Tool::Brush, &w, &w, &mut same, Ctx::default()).is_empty());
    assert_same_columns(&before, &same);
}

#[test]
fn unknown_columns_survive_and_missing_ones_are_not_added() {
    let sut = synth::sut(5, 1).unwrap();
    let before = sut.subtool.variant.clone().unwrap();
    let w = read(&sut, Tool::Brush);
    let mut now = w.clone();
    now.brush.size = 77.0;
    now.brush.opacity = 0.25;
    now.brush.hardness = 0.1;
    let mut after = before.clone();
    mapping::write_changed(Tool::Brush, &w, &now, &mut after, Ctx::default());
    assert_eq!(after.row.len(), before.row.len(), "no column added");
    assert_eq!(after.real(k::BRUSH_SIZE), Some(77.0));
    assert_eq!(after.int(k::OPACITY), Some(25));
    for name in ["SyntheticUnknownText", "SyntheticUnknownBlob", "SyntheticUnknownNull", k::BRUSH_PATTERN_IMAGES, k::SHOW_PARAM] {
        assert!(before.get(name).unwrap().same(after.get(name).unwrap()), "{name}");
    }
    let mut written = sut.clone();
    written.subtool.variant = Some(after);
    Sut::from_bytes(&written.to_bytes().unwrap(), &Limits::default()).unwrap();
    // The synthetic tip is read as the brush's tip.
    assert!(w.brush.tip.bitmap().is_some());
    let kept = mapping::kept_columns(Some(Tool::Brush), &before);
    assert!(kept.contains(&"SyntheticUnknownText".to_string()) && !kept.contains(&k::BRUSH_SIZE.to_string()));
}

#[test]
fn sizes_in_physical_units_convert_at_the_resolution() {
    let sut = common::sut("mm pen", (11, 11), &[(k::BRUSH_SIZE, Value::Real(1.0)), (k::BRUSH_SIZE_UNIT, Value::Integer(1))]);
    let v = sut.subtool.variant.as_ref().unwrap();
    let ctx = Ctx { ppi: 254.0 };
    let w = mapping::read(&Source { variant: v, materials: &[], ctx }, Tool::Brush, Working::default());
    assert!((w.brush.size - 10.0).abs() < 1e-4);
    let mut now = w.clone();
    now.brush.size = 20.0;
    let mut after = v.clone();
    mapping::write_changed(Tool::Brush, &w, &now, &mut after, ctx);
    assert!((after.real(k::BRUSH_SIZE).unwrap() - 2.0).abs() < 1e-6);
    assert_eq!(after.int(k::BRUSH_SIZE_UNIT), Some(1));
}

#[test]
fn the_reverse_direction_writes_a_sub_tool_that_reads_back() {
    let mut w = Working::default();
    w.brush.size = 42.0;
    w.brush.opacity = 0.5;
    w.brush.shape_dynamics.enabled = true;
    w.brush.shape_dynamics.size =
        photocraft_engine::paint::brush::Dynamic { minimum: 0.2, ..photocraft_engine::paint::brush::Dynamic::controlled(Control::PenPressure) };
    for t in [Tool::Brush, Tool::Pencil, Tool::Eraser, Tool::MixerBrush, Tool::PaintBucket, Tool::Move, Tool::Type] {
        let sut = reverse::new_sut("Made in PhotoCraft", t, &w, Ctx::default(), 9).unwrap();
        let back = Sut::from_bytes(&sut.to_bytes().unwrap(), &Limits::default()).unwrap();
        assert_eq!(tool::choose_subtool(&back.subtool).tool(), Some(t), "{t:?}");
        if mapping::Scope::Brush.applies(t) {
            let b = read(&back, t).brush;
            assert_eq!(b.size, 42.0);
            assert!((b.opacity - 0.5).abs() < 1e-6);
            assert_eq!(b.shape_dynamics.size.control, Control::PenPressure);
            assert!((b.shape_dynamics.size.minimum - 0.2).abs() < 1e-6);
        }
    }
    assert!(reverse::new_sut("x", Tool::Crop, &w, Ctx::default(), 1).is_err());
}

#[test]
fn hostile_values_read_without_panicking() {
    let hostile = [
        Value::Real(f64::NAN),
        Value::Real(f64::INFINITY),
        Value::Real(-1e300),
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Text("x".into()),
        Value::Blob(vec![0xff; 7]),
    ];
    let columns: Vec<String> = reverse::variant_columns().into_iter().map(|(n, _)| n).filter(|n| n != "_PW_ID" && n != "VariantID").collect();
    for value in &hostile {
        let values: Vec<(&str, Value)> = columns.iter().map(|c| (c.as_str(), value.clone())).collect();
        let sut = common::sut("Hostile", (10, 10), &values);
        for t in Tool::ALL {
            let w = read(&sut, t);
            assert!(w.brush.size.is_finite() && w.brush.size >= 0.5, "{value:?}");
            let mut v = sut.subtool.variant.clone().unwrap();
            mapping::write_all(t, &w, &mut v, Ctx::default());
        }
    }
}

/// Real exported sub tools: every one maps or is reported, and a full write keeps every value.
#[test]
fn real_sub_tools_round_trip() {
    let Some(dir) = std::env::var_os("CLIPPY_CSP_FIXTURES").map(std::path::PathBuf::from).filter(|d| d.is_dir()) else { return };
    let mut paths: Vec<_> = std::fs::read_dir(dir.join("Sut-Exporte")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "sut"));
    paths.sort();
    for path in paths {
        let sut = Sut::open(&path).unwrap();
        let choice = tool::choose_subtool(&sut.subtool);
        eprintln!("{}: In/Out {:?}/{:?} → {choice:?}", path.display(), sut.subtool.input_op(), sut.subtool.output_op());
        let Some(t) = choice.tool() else { continue };
        let before = sut.subtool.variant.clone().unwrap();
        let w = read(&sut, t);
        let mut after = before.clone();
        mapping::write_all(t, &w, &mut after, Ctx::default());
        for cell in before.row.cells() {
            // A NULL column takes PhotoCraft's value on a full write; every value stays.
            if !cell.value.is_null() {
                assert!(cell.value.same(after.get(&cell.name).unwrap()), "{}: {}", path.display(), cell.name);
            }
        }
    }
}
