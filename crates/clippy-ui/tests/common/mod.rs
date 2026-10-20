//! Synthetic sub tools for the mapping tests: generated here, no Clip Studio Paint data.
#![allow(dead_code)]

use photocraft_clippy_sut::{Curve, Effector, Source, Sources, Sut, Value, Variant, known as k};
use photocraft_clippy_ui::mapping::{Ctx, Working, reverse};
use photocraft_ui_egui::state::Tool;

pub fn effector(active: &[Source], minimums: &[(Source, i32)], pressure: Option<Vec<(f64, f64)>>) -> Value {
    let mut e = Effector::new(Sources(0x1F0));
    e.set_active(active.iter().fold(Sources(0), |s, x| s.with(*x, true)));
    for (s, m) in minimums {
        e.set_minimum(*s, *m);
    }
    e.set_pressure_curve(pressure.map(Curve::new));
    Value::Blob(e.encode())
}

/// The G-pen's pressure curve: thin until half pressure, then quickly full.
pub fn gpen_curve() -> Vec<(f64, f64)> {
    vec![(0.0, 0.0), (0.4, 0.15), (0.7, 0.6), (1.0, 1.0)]
}

/// A sub tool with In/Out `ops`, named `name`, every mapped column `NULL` but `values`.
pub fn sut(name: &str, ops: (i64, i64), values: &[(&str, Value)]) -> Sut {
    let mut sut = reverse::new_sut(name, Tool::Brush, &Working::default(), Ctx::default(), 1).unwrap();
    sut.subtool.node.set("NodeInputOp", Value::Integer(ops.0));
    sut.subtool.node.set("NodeOutputOp", Value::Integer(ops.1));
    for v in [sut.subtool.variant.as_mut().unwrap(), sut.subtool.init_variant.as_mut().unwrap()] {
        clear(v);
        for (c, x) in values {
            assert!(v.row.contains(c), "the template has no column {c}");
            v.set(c, x.clone());
        }
    }
    sut
}

fn clear(v: &mut Variant) {
    let names: Vec<String> = v.row.cells().iter().map(|c| c.name.clone()).filter(|n| n != "VariantID").collect();
    for n in names {
        v.set(&n, Value::Null);
    }
}

/// A G-pen: 12.5 px, pressure-driven size with a curve and an 18 % minimum.
pub fn gpen() -> Sut {
    sut(
        "G-Pen",
        (11, 11),
        &[
            (k::BRUSH_SIZE, Value::Real(12.5)),
            (k::BRUSH_SIZE_UNIT, Value::Integer(0)),
            (k::BRUSH_SIZE_EFFECTOR, effector(&[Source::Pressure], &[(Source::Pressure, 18)], Some(gpen_curve()))),
            (k::OPACITY, Value::Integer(100)),
            (k::ANTI_ALIAS, Value::Integer(2)),
            (k::BRUSH_HARDNESS, Value::Integer(100)),
            (k::COMPOSITE_MODE, Value::Integer(0)),
            (k::BRUSH_INTERVAL, Value::Real(8.0)),
            (k::BRUSH_AUTO_INTERVAL_TYPE, Value::Integer(0)),
            (k::FLICKER_REDUCTION, Value::Integer(12)),
        ],
    )
}

/// Every brush column with a value of its own, many away from PhotoCraft's defaults.
pub fn rich_brush_values() -> Vec<(&'static str, Value)> {
    vec![
        (k::BRUSH_SIZE, Value::Real(12.5)),
        (k::BRUSH_SIZE_UNIT, Value::Integer(0)),
        (k::BRUSH_SIZE_EFFECTOR, effector(&[Source::Pressure, Source::Velocity], &[(Source::Pressure, 18), (Source::Velocity, 40)], Some(gpen_curve()))),
        (k::BRUSH_AT_LEAST_1_PIXEL, Value::Integer(0)),
        (k::BRUSH_SIZE_SYNC_VIEW_SCALE, Value::Integer(1)),
        (k::OPACITY, Value::Integer(87)),
        (k::BRUSH_OPACITY_EFFECTOR, effector(&[Source::Tilt], &[(Source::Tilt, 30)], None)),
        (k::COMPOSITE_MODE, Value::Integer(10)),
        (k::BRUSH_MIX_COLOR, Value::Integer(100)),
        (k::BRUSH_MIX_ALPHA, Value::Integer(100)),
        (k::BRUSH_MIX_COLOR_EXTENSION, Value::Integer(0)),
        (k::BRUSH_HUE_CHANGE, Value::Integer(30)),
        (k::BRUSH_SATURATION_CHANGE, Value::Integer(15)),
        (k::BRUSH_VALUE_CHANGE, Value::Integer(5)),
        (k::BRUSH_SUB_COLOR, Value::Integer(20)),
        (k::BRUSH_CHANGE_STROKE_COLOR, Value::Integer(1)),
        (k::ANTI_ALIAS, Value::Integer(3)),
        (k::BRUSH_USE_PATTERN_IMAGE, Value::Integer(0)),
        (k::BRUSH_PATTERN_ORDER_TYPE, Value::Integer(1)),
        (k::BRUSH_HARDNESS, Value::Integer(70)),
        (k::BRUSH_THICKNESS, Value::Integer(65)),
        (k::BRUSH_THICKNESS_EFFECTOR, effector(&[Source::Random], &[(Source::Random, 50)], None)),
        (k::BRUSH_VERTICAL_THICKNESS, Value::Integer(1)),
        (k::BRUSH_ROTATION, Value::Real(30.0)),
        (k::BRUSH_ROTATION_EFFECTOR, effector(&[Source::Tilt, Source::Random], &[], None)),
        (k::BRUSH_ROTATION_RANDOM_SCALE, Value::Integer(25)),
        (k::BRUSH_PATTERN_REVERSE_HORIZONTAL, Value::Integer(0)),
        (k::BRUSH_PATTERN_REVERSE_VERTICAL, Value::Integer(1)),
        (k::BRUSH_FLOW, Value::Integer(90)),
        (k::BRUSH_FLOW_EFFECTOR, effector(&[Source::Velocity], &[(Source::Velocity, 10)], None)),
        (k::BRUSH_USE_SPRAY, Value::Integer(1)),
        (k::BRUSH_SPRAY_SIZE, Value::Real(25.0)),
        (k::BRUSH_SPRAY_DENSITY, Value::Integer(50)),
        (k::BRUSH_SPRAY_SIZE_EFFECTOR, effector(&[Source::Pressure], &[(Source::Pressure, 0)], Some(vec![(0.0, 0.0), (1.0, 1.0)]))),
        (k::BRUSH_INTERVAL, Value::Real(8.0)),
        (k::BRUSH_AUTO_INTERVAL_TYPE, Value::Integer(0)),
        (k::BRUSH_CONTINUOUS_PLOT, Value::Integer(0)),
        (k::BRUSH_BLEND_PATTERN_BY_DARKEN, Value::Integer(1)),
        (k::TEXTURE_DENSITY, Value::Integer(60)),
        (k::TEXTURE_REVERSE_DENSITY, Value::Integer(1)),
        (k::TEXTURE_SCALE, Value::Real(150.0)),
        (k::TEXTURE_ROTATE, Value::Real(15.0)),
        (k::TEXTURE_BRIGHTNESS, Value::Integer(-20)),
        (k::TEXTURE_CONTRAST, Value::Integer(10)),
        (k::TEXTURE_COMPOSITE_MODE, Value::Integer(3)),
        (k::TEXTURE_FOR_PLOT, Value::Integer(1)),
        (k::USE_DUAL_BRUSH, Value::Integer(1)),
        (k::DUAL_SIZE, Value::Real(50.0)),
        (k::DUAL_BRUSH_COMPOSITE_MODE, Value::Integer(2)),
        (k::BRUSH_USE_WATER_EDGE, Value::Integer(1)),
        (k::BRUSH_WATER_EDGE_RADIUS, Value::Real(3.0)),
        (k::BRUSH_WATER_EDGE_ALPHA_POWER, Value::Integer(40)),
        (k::FLICKER_REDUCTION, Value::Integer(12)),
        (k::FLICKER_REDUCTION_BY_SPEED, Value::Integer(1)),
        (k::FLICKER_REDUCTION_BY_SPEED_TYPE, Value::Integer(1)),
        (k::STICKNESS, Value::Integer(3)),
        (k::BRUSH_ENABLE_SNAP, Value::Integer(0)),
        (k::BRUSH_USE_IN, Value::Integer(1)),
        (k::BRUSH_USE_OUT, Value::Integer(1)),
        (k::BRUSH_IN_LENGTH, Value::Real(40.0)),
        (k::BRUSH_OUT_LENGTH, Value::Real(25.0)),
        (k::BRUSH_IN_OUT_BY_SPEED, Value::Integer(1)),
    ]
}

/// A scratch folder, removed again when dropped.
pub struct TempDir(pub std::path::PathBuf);

impl std::ops::Deref for TempDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn temp_dir(name: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!("clippy-m8-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}
