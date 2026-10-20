//! Brush columns ↔ `BrushSettings`. Units as observed in Clip Studio Paint 1.5 files (and
//! assumed where noted, `CLIPPY.md` › Mapping): percentages 0–100, angles in degrees, lengths in
//! pixels; `BrushSize` in the unit of `BrushSizeUnit`.

use photocraft_clippy_sut::{Source, Sources, Variant, known as k};
use photocraft_engine::paint::GrayTile;
use photocraft_engine::paint::brush::{Control, Dynamic, MAX_BRUSH_SIZE, MAX_SCATTER, Pattern, SpeedSmoothing, TipOrder, TipShape};
use serde_json::{Value, json};

use super::cols::{effector, effector_for_write, flag, int, num, num_in, percent, put_flag, put_int, put_num, put_percent};
use super::effector::{self as eff, to_dynamic, write_dynamic};
use super::{Rule, Scope, Source as Src, Working, blend};

/// Pixels per `BrushSizeUnit` step at `ppi`: 0 px, 1 mm, 2 cm, 3 inch, 4 pt, 5 Q (an assumption
/// for every unit but px). Unknown units read as pixels.
fn px_per_unit(unit: i64, ppi: f64) -> f64 {
    let ppi = if ppi.is_finite() && ppi > 0.0 { ppi } else { 72.0 };
    match unit {
        1 => ppi / 25.4,
        2 => ppi / 2.54,
        3 => ppi,
        4 => ppi / 72.0,
        5 => ppi / 101.6,
        _ => 1.0,
    }
}

fn read_effector(v: &Variant, column: &str) -> Option<Dynamic> {
    effector(v, column).map(|e| to_dynamic(&e))
}

/// Write `d` into the effector of `column` unless it already reads the same.
fn write_effector(v: &mut Variant, column: &str, d: &Dynamic) {
    let Some(mut e) = effector_for_write(v, column) else { return };
    if !eff::same(&to_dynamic(&e), d) {
        write_dynamic(d, &mut e);
        v.set_effector(column, &e);
    }
}

/// A dynamic as the brush paints it: nothing while its section is off; the options bar's
/// pressure button stands for a pressure control.
fn effective(section_on: bool, d: &Dynamic, pressure_button: bool) -> Dynamic {
    if section_on && d.is_active() {
        d.clone()
    } else if pressure_button {
        Dynamic::controlled(Control::PenPressure)
    } else {
        Dynamic::default()
    }
}

fn size_dynamic(w: &Working) -> Dynamic {
    let b = &w.brush;
    effective(b.shape_dynamics.enabled, &b.shape_dynamics.size, b.pressure_size)
}

fn opacity_dynamic(w: &Working) -> Dynamic {
    let b = &w.brush;
    effective(b.transfer.enabled, &b.transfer.opacity, b.pressure_opacity)
}

/// Settle the brush after reading: the sections follow their dynamics, and pen pressure comes
/// from the pen settings only (PhotoCraft's options bar buttons start off).
pub(super) fn finish(b: &mut photocraft_engine::BrushSettings) {
    let sd = &mut b.shape_dynamics;
    sd.enabled = sd.size.is_active() || sd.angle.is_active() || sd.roundness.is_active();
    b.transfer.enabled = b.transfer.opacity.is_active() || b.transfer.flow.is_active();
    b.pressure_size = false;
    b.pressure_opacity = false;
    b.spacing_enabled = true;
}

/// A brush tip or texture from the materials embedded in the `.sut`, as coverage. `None` when the
/// list or the material is missing or doesn't decode (desktop only: the archives are tar).
#[cfg(not(target_arch = "wasm32"))]
fn tiles(src: &Src<'_>, column: &str) -> Vec<GrayTile> {
    use photocraft_clippy_sut::{Limits, layer};
    // Tips large enough for any brush, small enough to keep a malformed file from taking memory.
    let limits = Limits { max_tip_dimension: 8192, max_tip_pixels: 1 << 24, ..Limits::default() };
    let Some(Ok(refs)) = src.variant.material_refs(column) else { return Vec::new() };
    refs.entries
        .iter()
        .take(photocraft_engine::paint::brush::MAX_TIPS)
        .filter_map(|r| {
            let material = src.materials.iter().find(|m| m.catalog_path().is_some_and(|c| r.path.starts_with(c)))?;
            let archive = material.archive(&limits).ok()?;
            let bytes = r.file_name().and_then(|f| archive.get(&format!("data/{f}"))).or_else(|| archive.tip_layer())?;
            let tip = layer::decode(bytes, &limits).ok()?;
            let coverage = tip.coverage();
            if tip.width == 0 || tip.height == 0 || coverage.len() != (tip.width as usize).saturating_mul(tip.height as usize) {
                return None;
            }
            let w = tip.width;
            Some(GrayTile::from_fn(tip.width, tip.height, |x, y| {
                coverage.get((y as usize).saturating_mul(w as usize).saturating_add(x as usize)).map_or(0.0, |&c| f32::from(c) / 255.0)
            }))
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn tiles(_: &Src<'_>, _: &str) -> Vec<GrayTile> {
    Vec::new()
}

/// `BrushPatternOrderType` (assumed: 0 repeat, 1 round trip, 2 random).
fn order_of(n: i64) -> TipOrder {
    match n {
        1 => TipOrder::RoundTrip,
        2 => TipOrder::Random,
        _ => TipOrder::Repeat,
    }
}

fn order_number(o: TipOrder) -> i64 {
    match o {
        TipOrder::Repeat => 0,
        TipOrder::RoundTrip => 1,
        TipOrder::Random => 2,
    }
}

/// The spacing the interval columns stand for (`BrushAutoIntervalType`, assumed: 0 the
/// interval in percent of the size, 1 narrow, 2 normal, 3 wide).
fn spacing_of(v: &Variant) -> Option<f32> {
    match int(v, k::BRUSH_AUTO_INTERVAL_TYPE).unwrap_or(0) {
        0 => num_in(v, k::BRUSH_INTERVAL, 1.0, 1000.0).map(|p| p / 100.0),
        1 => Some(0.05),
        2 => Some(0.1),
        3 => Some(0.25),
        _ => None,
    }
}

/// The angle dynamic of the direction pen settings: pressure or tilt turns the tip, random turns
/// it by up to `BrushRotationRandomScale` % of ±180°. Only a blob column is a pen setting (some
/// files store an integer there, kept as it is).
fn angle_dynamic(v: &Variant) -> Option<Dynamic> {
    let e = effector(v, k::BRUSH_ROTATION_EFFECTOR)?;
    let active = e.active();
    let control = if active.contains(Source::Tilt) {
        Control::PenTilt
    } else if active.contains(Source::Pressure) {
        Control::PenPressure
    } else {
        Control::Off
    };
    let jitter = if active.contains(Source::Random) { percent(v, k::BRUSH_ROTATION_RANDOM_SCALE).unwrap_or(1.0) } else { 0.0 };
    Some(Dynamic { control, jitter, ..Dynamic::default() })
}

fn angle_key(d: &Dynamic) -> Value {
    let control = if matches!(d.control, Control::PenTilt | Control::PenPressure) { format!("{:?}", d.control) } else { String::new() };
    json!([control, (d.jitter.clamp(0.0, 1.0) * 100.0).round()])
}

fn write_angle_dynamic(v: &mut Variant, d: &Dynamic) {
    let Some(mut e) = effector_for_write(v, k::BRUSH_ROTATION_EFFECTOR) else { return };
    if angle_dynamic(v).is_some_and(|now| angle_key(&now) == angle_key(d)) {
        return;
    }
    let mut active = Sources(e.active().0 & !(Source::Pressure.bit() | Source::Tilt.bit() | Source::Random.bit()));
    match d.control {
        Control::PenTilt => active = active.with(Source::Tilt, true),
        Control::PenPressure => active = active.with(Source::Pressure, true),
        _ => {}
    }
    if d.jitter > 0.0 {
        active = active.with(Source::Random, true);
        put_percent(v, k::BRUSH_ROTATION_RANDOM_SCALE, d.jitter.clamp(0.0, 1.0));
    }
    e.set_active(active);
    v.set_effector(k::BRUSH_ROTATION_EFFECTOR, &e);
}

macro_rules! flag_rule {
    ($id:literal, $col:expr, $($field:ident).+) => {
        Rule {
            id: $id,
            scope: Scope::Brush,
            columns: &[$col],
            read: |s, w| {
                if let Some(on) = flag(s.variant, $col) {
                    w.brush.$($field).+ = on;
                }
            },
            write: |w, v, _| put_flag(v, $col, w.brush.$($field).+),
            key: |w| json!(w.brush.$($field).+),
        }
    };
}

macro_rules! percent_rule {
    ($id:literal, $col:expr, $($field:ident).+) => {
        Rule {
            id: $id,
            scope: Scope::Brush,
            columns: &[$col],
            read: |s, w| {
                if let Some(p) = percent(s.variant, $col) {
                    w.brush.$($field).+ = p;
                }
            },
            write: |w, v, _| put_percent(v, $col, w.brush.$($field).+),
            key: |w| json!(w.brush.$($field).+),
        }
    };
}

macro_rules! effector_rule {
    ($id:literal, $col:expr, $get:expr, $set:expr) => {
        Rule {
            id: $id,
            scope: Scope::Brush,
            columns: &[$col],
            read: |s, w| {
                if let Some(d) = read_effector(s.variant, $col) {
                    ($set)(w, d);
                }
            },
            write: |w, v, _| write_effector(v, $col, &($get)(w)),
            key: |w| eff::key(&($get)(w)),
        }
    };
}

pub(super) const RULES: [Rule; 30] = [
    Rule {
        id: "size",
        scope: Scope::Brush,
        columns: &[k::BRUSH_SIZE, k::BRUSH_SIZE_UNIT],
        read: |s, w| {
            if let Some(size) = num(s.variant, k::BRUSH_SIZE) {
                let px = size * px_per_unit(int(s.variant, k::BRUSH_SIZE_UNIT).unwrap_or(0), s.ctx.ppi);
                w.brush.size = px.clamp(0.5, f64::from(MAX_BRUSH_SIZE)) as f32;
            }
        },
        write: |w, v, ctx| {
            let value = f64::from(w.brush.size) / px_per_unit(int(v, k::BRUSH_SIZE_UNIT).unwrap_or(0), ctx.ppi);
            put_num(v, k::BRUSH_SIZE, value, 1e-5 * value.abs().max(1.0));
        },
        key: |w| json!(w.brush.size),
    },
    effector_rule!("sizeDynamics", k::BRUSH_SIZE_EFFECTOR, size_dynamic, |w: &mut Working, d| w.brush.shape_dynamics.size = d),
    flag_rule!("atLeastOnePixel", k::BRUSH_AT_LEAST_1_PIXEL, min_one_px),
    flag_rule!("sizeFollowsView", k::BRUSH_SIZE_SYNC_VIEW_SCALE, size_follows_view),
    percent_rule!("opacity", k::OPACITY, opacity),
    effector_rule!("opacityDynamics", k::BRUSH_OPACITY_EFFECTOR, opacity_dynamic, |w: &mut Working, d| w.brush.transfer.opacity = d),
    Rule {
        id: "blendMode",
        scope: Scope::Brush,
        columns: &[k::COMPOSITE_MODE],
        read: |s, w| {
            if let Some(mode) = int(s.variant, k::COMPOSITE_MODE).and_then(blend::mode_of) {
                w.brush.mode = mode;
            }
        },
        write: |w, v, _| {
            // A stored number that reads as this mode (Glow Dodge for Color Dodge) stays.
            if int(v, k::COMPOSITE_MODE).and_then(blend::mode_of) != Some(w.brush.mode)
                && let Some(n) = blend::number_of(w.brush.mode)
            {
                put_int(v, k::COMPOSITE_MODE, n);
            }
        },
        key: |w| json!(format!("{:?}", w.brush.mode)),
    },
    Rule {
        id: "colorMixing",
        scope: Scope::Mixer,
        columns: &[k::BRUSH_MIX_COLOR, k::BRUSH_MIX_ALPHA, k::BRUSH_MIX_COLOR_EXTENSION],
        read: |s, w| {
            // Approximation: amount of paint → the canvas share (Mix), density of paint → Wet,
            // colour stretch → Load.
            let m = &mut w.brush.mixer;
            if let Some(p) = percent(s.variant, k::BRUSH_MIX_COLOR) {
                m.mix = 1.0 - p;
            }
            if let Some(p) = percent(s.variant, k::BRUSH_MIX_ALPHA) {
                m.wet = 1.0 - p;
            }
            if let Some(p) = percent(s.variant, k::BRUSH_MIX_COLOR_EXTENSION) {
                m.load = p;
            }
        },
        write: |w, v, _| {
            let m = &w.brush.mixer;
            put_percent(v, k::BRUSH_MIX_COLOR, 1.0 - m.mix.clamp(0.0, 1.0));
            put_percent(v, k::BRUSH_MIX_ALPHA, 1.0 - m.wet.clamp(0.0, 1.0));
            put_percent(v, k::BRUSH_MIX_COLOR_EXTENSION, m.load);
        },
        key: |w| json!([w.brush.mixer.mix, w.brush.mixer.wet, w.brush.mixer.load]),
    },
    Rule {
        id: "colorJitter",
        scope: Scope::Brush,
        columns: &[k::BRUSH_HUE_CHANGE, k::BRUSH_SATURATION_CHANGE, k::BRUSH_VALUE_CHANGE, k::BRUSH_SUB_COLOR, k::BRUSH_CHANGE_STROKE_COLOR],
        read: |s, w| {
            let v = s.variant;
            let cd = &mut w.brush.color_dynamics;
            // Hue in degrees (0–180, either way), the others in percent.
            if let Some(h) = num_in(v, k::BRUSH_HUE_CHANGE, 0.0, 180.0) {
                cd.hue_jitter = h / 180.0;
            }
            if let Some(p) = percent(v, k::BRUSH_SATURATION_CHANGE) {
                cd.saturation_jitter = p;
            }
            if let Some(p) = percent(v, k::BRUSH_VALUE_CHANGE) {
                cd.brightness_jitter = p;
            }
            if let Some(p) = percent(v, k::BRUSH_SUB_COLOR) {
                cd.fg_bg.jitter = p;
            }
            if let Some(per_stroke) = flag(v, k::BRUSH_CHANGE_STROKE_COLOR) {
                cd.per_tip = !per_stroke;
            }
            cd.enabled = cd.hue_jitter > 0.0 || cd.saturation_jitter > 0.0 || cd.brightness_jitter > 0.0 || cd.fg_bg.jitter > 0.0;
        },
        write: |w, v, _| {
            let cd = &w.brush.color_dynamics;
            let on = |x: f32| if cd.enabled { x.clamp(0.0, 1.0) } else { 0.0 };
            put_num(v, k::BRUSH_HUE_CHANGE, f64::from(on(cd.hue_jitter)) * 180.0, 1e-3);
            put_percent(v, k::BRUSH_SATURATION_CHANGE, on(cd.saturation_jitter));
            put_percent(v, k::BRUSH_VALUE_CHANGE, on(cd.brightness_jitter));
            put_percent(v, k::BRUSH_SUB_COLOR, on(cd.fg_bg.jitter));
            put_flag(v, k::BRUSH_CHANGE_STROKE_COLOR, !cd.per_tip);
        },
        key: |w| {
            let cd = &w.brush.color_dynamics;
            json!([cd.enabled, cd.hue_jitter, cd.saturation_jitter, cd.brightness_jitter, cd.fg_bg.jitter, cd.per_tip])
        },
    },
    Rule {
        id: "antiAliasing",
        scope: Scope::Brush,
        columns: &[k::ANTI_ALIAS],
        read: |s, w| {
            if let Some(level) = int(s.variant, k::ANTI_ALIAS) {
                let level = level.clamp(0, 3) as u8;
                w.brush.aa_level = Some(level);
                w.brush.aliased = level == 0;
            }
        },
        write: |w, v, _| {
            let level = w.brush.aa_level.map_or(if w.brush.aliased { 0 } else { 2 }, |l| l.min(3));
            put_int(v, k::ANTI_ALIAS, i64::from(level));
        },
        key: |w| json!([w.brush.is_aliased(), w.brush.aa_level]),
    },
    Rule {
        id: "tips",
        scope: Scope::Brush,
        columns: &[k::BRUSH_USE_PATTERN_IMAGE, k::BRUSH_PATTERN_IMAGES, k::BRUSH_PATTERN_ORDER_TYPE],
        read: |s, w| {
            if let Some(n) = int(s.variant, k::BRUSH_PATTERN_ORDER_TYPE) {
                w.brush.tip_order = order_of(n);
            }
            // Without the switch column a tip list alone means tips.
            if flag(s.variant, k::BRUSH_USE_PATTERN_IMAGE) != Some(false) {
                let mut tips = tiles(s, k::BRUSH_PATTERN_IMAGES).into_iter().map(TipShape::Sampled);
                if let Some(first) = tips.next() {
                    w.brush.tip = first;
                    w.brush.tips = tips.collect();
                }
            }
        },
        // The tips themselves stay in the .sut (writing materials is the stock's job); the
        // order is a setting.
        write: |w, v, _| put_int(v, k::BRUSH_PATTERN_ORDER_TYPE, order_number(w.brush.tip_order)),
        key: |w| json!(order_number(w.brush.tip_order)),
    },
    percent_rule!("hardness", k::BRUSH_HARDNESS, hardness),
    Rule {
        id: "thickness",
        scope: Scope::Brush,
        columns: &[k::BRUSH_THICKNESS, k::BRUSH_THICKNESS_EFFECTOR],
        read: |s, w| {
            if let Some(p) = percent(s.variant, k::BRUSH_THICKNESS) {
                w.brush.roundness = p.max(0.01);
            }
            if let Some(d) = read_effector(s.variant, k::BRUSH_THICKNESS_EFFECTOR) {
                w.brush.shape_dynamics.roundness = d;
            }
        },
        write: |w, v, _| {
            put_percent(v, k::BRUSH_THICKNESS, w.brush.roundness);
            let sd = &w.brush.shape_dynamics;
            write_effector(v, k::BRUSH_THICKNESS_EFFECTOR, &effective(sd.enabled, &sd.roundness, false));
        },
        key: |w| {
            let sd = &w.brush.shape_dynamics;
            json!([w.brush.roundness, eff::key(&effective(sd.enabled, &sd.roundness, false))])
        },
    },
    Rule {
        id: "direction",
        scope: Scope::Brush,
        columns: &[k::BRUSH_ROTATION, k::BRUSH_VERTICAL_THICKNESS, k::BRUSH_ROTATION_EFFECTOR, k::BRUSH_ROTATION_RANDOM_SCALE],
        read: |s, w| {
            // Thickness applied vertically squashes the tip across: the same as turning it 90°.
            let turn = if flag(s.variant, k::BRUSH_VERTICAL_THICKNESS) == Some(true) { 90.0 } else { 0.0 };
            if let Some(deg) = num_in(s.variant, k::BRUSH_ROTATION, -3600.0, 3600.0) {
                w.brush.angle = deg + turn;
            }
            if let Some(d) = angle_dynamic(s.variant) {
                w.brush.shape_dynamics.angle = d;
            }
        },
        write: |w, v, _| {
            let turn = if flag(v, k::BRUSH_VERTICAL_THICKNESS) == Some(true) { 90.0 } else { 0.0 };
            put_num(v, k::BRUSH_ROTATION, f64::from(w.brush.angle) - turn, 1e-3);
            let sd = &w.brush.shape_dynamics;
            write_angle_dynamic(v, &effective(sd.enabled, &sd.angle, false));
        },
        key: |w| {
            let sd = &w.brush.shape_dynamics;
            json!([w.brush.angle, angle_key(&effective(sd.enabled, &sd.angle, false))])
        },
    },
    flag_rule!("flipHorizontal", k::BRUSH_PATTERN_REVERSE_HORIZONTAL, flip_x),
    flag_rule!("flipVertical", k::BRUSH_PATTERN_REVERSE_VERTICAL, flip_y),
    percent_rule!("density", k::BRUSH_FLOW, flow),
    effector_rule!(
        "densityDynamics",
        k::BRUSH_FLOW_EFFECTOR,
        |w: &Working| effective(w.brush.transfer.enabled, &w.brush.transfer.flow, false),
        |w: &mut Working, d| w.brush.transfer.flow = d
    ),
    Rule {
        id: "spray",
        scope: Scope::Brush,
        columns: &[k::BRUSH_USE_SPRAY, k::BRUSH_SPRAY_SIZE, k::BRUSH_SPRAY_DENSITY, k::BRUSH_SPRAY_SIZE_EFFECTOR],
        read: |s, w| {
            // Approximation: the spray's diameter (pixels) as scatter of the brush's own size,
            // its particle density (1–100) as up to 16 dabs per step.
            let v = s.variant;
            let sc = &mut w.brush.scattering;
            if let Some(on) = flag(v, k::BRUSH_USE_SPRAY) {
                sc.enabled = on;
                sc.both_axes = true;
            }
            if let Some(px) = num_in(v, k::BRUSH_SPRAY_SIZE, 0.0, f64::from(MAX_BRUSH_SIZE)) {
                sc.scatter.jitter = (px / w.brush.size.max(0.5)).clamp(0.0, MAX_SCATTER);
            }
            if let Some(d) = num_in(v, k::BRUSH_SPRAY_DENSITY, 0.0, 100.0) {
                sc.count = ((d / 100.0 * 16.0).round() as u32).clamp(1, 16);
            }
            if let Some(d) = read_effector(v, k::BRUSH_SPRAY_SIZE_EFFECTOR) {
                sc.scatter = Dynamic { jitter: sc.scatter.jitter, ..d };
            }
        },
        write: |w, v, _| {
            let sc = &w.brush.scattering;
            put_flag(v, k::BRUSH_USE_SPRAY, sc.enabled);
            if sc.enabled {
                let px = f64::from(sc.scatter.jitter.clamp(0.0, MAX_SCATTER) * w.brush.size);
                put_num(v, k::BRUSH_SPRAY_SIZE, px, 1e-3 * px.max(1.0));
                if (num_in(v, k::BRUSH_SPRAY_DENSITY, 0.0, 100.0).map(|d| ((d / 100.0 * 16.0).round() as u32).clamp(1, 16))) != Some(sc.count.clamp(1, 16)) {
                    put_num(v, k::BRUSH_SPRAY_DENSITY, f64::from(sc.count.clamp(1, 16)) * 100.0 / 16.0, 0.0);
                }
                let control = Dynamic { jitter: 0.0, ..sc.scatter.clone() };
                write_effector(v, k::BRUSH_SPRAY_SIZE_EFFECTOR, &control);
            }
        },
        key: |w| {
            let sc = &w.brush.scattering;
            json!([sc.enabled, sc.scatter.jitter, sc.count, eff::key(&Dynamic { jitter: 0.0, ..sc.scatter.clone() })])
        },
    },
    Rule {
        id: "spacing",
        scope: Scope::Brush,
        columns: &[k::BRUSH_INTERVAL, k::BRUSH_AUTO_INTERVAL_TYPE],
        read: |s, w| {
            if let Some(spacing) = spacing_of(s.variant) {
                w.brush.spacing = spacing;
            }
        },
        write: |w, v, _| {
            if spacing_of(v).is_some_and(|s| (s - w.brush.spacing).abs() < 1e-5) {
                return;
            }
            put_int(v, k::BRUSH_AUTO_INTERVAL_TYPE, 0);
            put_num(v, k::BRUSH_INTERVAL, f64::from(w.brush.spacing.clamp(0.01, 10.0)) * 100.0, 1e-3);
        },
        key: |w| json!(w.brush.spacing),
    },
    flag_rule!("continuousSpraying", k::BRUSH_CONTINUOUS_PLOT, build_up),
    flag_rule!("blendDarken", k::BRUSH_BLEND_PATTERN_BY_DARKEN, blend_darken),
    Rule {
        id: "texture",
        scope: Scope::Brush,
        columns: &[
            k::TEXTURE_IMAGE,
            k::TEXTURE_DENSITY,
            k::TEXTURE_DENSITY_EFFECTOR,
            k::TEXTURE_REVERSE_DENSITY,
            k::TEXTURE_SCALE,
            k::TEXTURE_ROTATE,
            k::TEXTURE_BRIGHTNESS,
            k::TEXTURE_CONTRAST,
            k::TEXTURE_COMPOSITE_MODE,
            k::TEXTURE_FOR_PLOT,
        ],
        read: |s, w| {
            let v = s.variant;
            let t = &mut w.brush.texture;
            if let Some(tile) = tiles(s, k::TEXTURE_IMAGE).into_iter().next() {
                t.pattern = Pattern::Tile(tile);
                t.enabled = true;
            }
            if let Some(p) = percent(v, k::TEXTURE_DENSITY) {
                t.depth = p;
            }
            if let Some(d) = read_effector(v, k::TEXTURE_DENSITY_EFFECTOR) {
                t.depth_jitter = d;
            }
            if let Some(on) = flag(v, k::TEXTURE_REVERSE_DENSITY) {
                t.invert = on;
            }
            if let Some(p) = num_in(v, k::TEXTURE_SCALE, 1.0, 1000.0) {
                t.scale = p / 100.0;
            }
            if let Some(deg) = num_in(v, k::TEXTURE_ROTATE, -3600.0, 3600.0) {
                t.rotation = deg;
            }
            if let Some(b) = num_in(v, k::TEXTURE_BRIGHTNESS, -100.0, 100.0) {
                t.brightness = b / 100.0;
            }
            if let Some(c) = num_in(v, k::TEXTURE_CONTRAST, -100.0, 100.0) {
                t.contrast = c / 100.0;
            }
            if let Some(mode) = int(v, k::TEXTURE_COMPOSITE_MODE).and_then(blend::mask_of) {
                t.mode = mode;
            }
            if let Some(on) = flag(v, k::TEXTURE_FOR_PLOT) {
                t.each_tip = on;
            }
        },
        write: |w, v, _| {
            let t = &w.brush.texture;
            put_percent(v, k::TEXTURE_DENSITY, t.depth);
            write_effector(v, k::TEXTURE_DENSITY_EFFECTOR, &t.depth_jitter);
            put_flag(v, k::TEXTURE_REVERSE_DENSITY, t.invert);
            put_num(v, k::TEXTURE_SCALE, f64::from(t.scale) * 100.0, 1e-3);
            put_num(v, k::TEXTURE_ROTATE, f64::from(t.rotation), 1e-3);
            put_num(v, k::TEXTURE_BRIGHTNESS, f64::from(t.brightness) * 100.0, 1e-3);
            put_num(v, k::TEXTURE_CONTRAST, f64::from(t.contrast) * 100.0, 1e-3);
            if int(v, k::TEXTURE_COMPOSITE_MODE).and_then(blend::mask_of) != Some(t.mode)
                && let Some(n) = blend::mask_number(t.mode)
            {
                put_int(v, k::TEXTURE_COMPOSITE_MODE, n);
            }
            put_flag(v, k::TEXTURE_FOR_PLOT, t.each_tip);
        },
        key: |w| {
            let t = &w.brush.texture;
            json!([t.depth, eff::key(&t.depth_jitter), t.invert, t.scale, t.rotation, t.brightness, t.contrast, format!("{:?}", t.mode), t.each_tip])
        },
    },
    Rule {
        id: "dualBrush",
        scope: Scope::Brush,
        columns: &[k::USE_DUAL_BRUSH, k::DUAL_PATTERN_IMAGES, k::DUAL_SIZE, k::DUAL_BRUSH_COMPOSITE_MODE, k::DUAL_TEXTURE_IMAGE],
        read: |s, w| {
            let v = s.variant;
            if let Some(on) = flag(v, k::USE_DUAL_BRUSH) {
                w.brush.dual_brush.enabled = on;
            }
            if let Some(tile) = tiles(s, k::DUAL_PATTERN_IMAGES).into_iter().next() {
                w.brush.dual_brush.tip = TipShape::Sampled(tile);
            }
            // The second tip's size in percent of the brush size.
            if let Some(p) = num_in(v, k::DUAL_SIZE, 0.0, 10_000.0) {
                w.brush.dual_brush.size = (w.brush.size * p / 100.0).clamp(0.5, MAX_BRUSH_SIZE);
            }
            if let Some(mode) = int(v, k::DUAL_BRUSH_COMPOSITE_MODE).and_then(blend::mask_of) {
                w.brush.dual_brush.mode = mode;
            }
            if let Some(tile) = tiles(s, k::DUAL_TEXTURE_IMAGE).into_iter().next() {
                w.brush.dual_brush.texture.pattern = Pattern::Tile(tile);
                w.brush.dual_brush.texture.enabled = true;
            }
        },
        write: |w, v, _| {
            let d = &w.brush.dual_brush;
            put_flag(v, k::USE_DUAL_BRUSH, d.enabled);
            let ratio = f64::from(d.size) / f64::from(w.brush.size.max(0.5)) * 100.0;
            put_num(v, k::DUAL_SIZE, ratio, 1e-3 * ratio.max(1.0));
            if int(v, k::DUAL_BRUSH_COMPOSITE_MODE).and_then(blend::mask_of) != Some(d.mode)
                && let Some(n) = blend::mask_number(d.mode)
            {
                put_int(v, k::DUAL_BRUSH_COMPOSITE_MODE, n);
            }
        },
        key: |w| {
            let d = &w.brush.dual_brush;
            // The share of the size, as stored (a resize that keeps it writes nothing).
            let share = (f64::from(d.size) / f64::from(w.brush.size.max(0.5)) * 1e4).round();
            json!([d.enabled, share, format!("{:?}", d.mode)])
        },
    },
    Rule {
        id: "watercolorEdge",
        scope: Scope::Brush,
        columns: &[k::BRUSH_USE_WATER_EDGE, k::BRUSH_WATER_EDGE_RADIUS, k::BRUSH_WATER_EDGE_ALPHA_POWER],
        read: |s, w| {
            // The edge's width in pixels as a share of the radius; the opacity effect as how much
            // paint the inside loses.
            let v = s.variant;
            if let Some(on) = flag(v, k::BRUSH_USE_WATER_EDGE) {
                w.brush.wet_edges = on;
            }
            if let Some(px) = num_in(v, k::BRUSH_WATER_EDGE_RADIUS, 0.0, f64::from(MAX_BRUSH_SIZE)) {
                w.brush.wet_edge.width = (px / (w.brush.size / 2.0).max(0.25)).clamp(0.01, 1.0);
            }
            if let Some(p) = percent(v, k::BRUSH_WATER_EDGE_ALPHA_POWER) {
                w.brush.wet_edge.interior = 1.0 - p;
            }
        },
        write: |w, v, _| {
            put_flag(v, k::BRUSH_USE_WATER_EDGE, w.brush.wet_edges);
            let px = f64::from(w.brush.wet_edge.width.clamp(0.0, 1.0) * w.brush.size / 2.0);
            put_num(v, k::BRUSH_WATER_EDGE_RADIUS, px, 1e-3 * px.max(1.0));
            put_percent(v, k::BRUSH_WATER_EDGE_ALPHA_POWER, 1.0 - w.brush.wet_edge.interior.clamp(0.0, 1.0));
        },
        key: |w| json!([w.brush.wet_edges, w.brush.wet_edge.width, w.brush.wet_edge.interior]),
    },
    percent_rule!("stabilization", k::FLICKER_REDUCTION, smoothing.amount),
    Rule {
        id: "stabilizationBySpeed",
        scope: Scope::Brush,
        columns: &[k::FLICKER_REDUCTION_BY_SPEED, k::FLICKER_REDUCTION_BY_SPEED_TYPE],
        read: |s, w| {
            // Assumed: type 0 strengthens the smoothing at low speed, 1 weakens it at high speed.
            if let Some(on) = flag(s.variant, k::FLICKER_REDUCTION_BY_SPEED) {
                let kind =
                    if int(s.variant, k::FLICKER_REDUCTION_BY_SPEED_TYPE) == Some(1) { SpeedSmoothing::ReduceFast } else { SpeedSmoothing::StrengthenSlow };
                w.brush.smoothing.by_speed = on.then_some(kind);
            }
        },
        write: |w, v, _| {
            put_flag(v, k::FLICKER_REDUCTION_BY_SPEED, w.brush.smoothing.by_speed.is_some());
            if let Some(kind) = w.brush.smoothing.by_speed {
                put_int(v, k::FLICKER_REDUCTION_BY_SPEED_TYPE, if kind == SpeedSmoothing::ReduceFast { 1 } else { 0 });
            }
        },
        key: |w| json!(format!("{:?}", w.brush.smoothing.by_speed)),
    },
    Rule {
        id: "stickiness",
        scope: Scope::Brush,
        columns: &[k::STICKNESS],
        read: |s, w| {
            if let Some(n) = int(s.variant, k::STICKNESS) {
                w.brush.smoothing.stickiness = n.clamp(0, i64::from(u8::MAX)) as u8;
            }
        },
        write: |w, v, _| put_int(v, k::STICKNESS, i64::from(w.brush.smoothing.stickiness)),
        key: |w| json!(w.brush.smoothing.stickiness),
    },
    Rule {
        id: "snapping",
        scope: Scope::Brush,
        columns: &[k::BRUSH_ENABLE_SNAP],
        read: |s, w| {
            if let Some(on) = flag(s.variant, k::BRUSH_ENABLE_SNAP) {
                w.snap = on;
            }
        },
        write: |w, v, _| put_flag(v, k::BRUSH_ENABLE_SNAP, w.snap),
        key: |w| json!(w.snap),
    },
    Rule {
        id: "startAndEnd",
        scope: Scope::Brush,
        columns: &[k::BRUSH_USE_IN, k::BRUSH_USE_OUT, k::BRUSH_IN_LENGTH, k::BRUSH_OUT_LENGTH, k::BRUSH_IN_OUT_BY_SPEED],
        read: |s, w| {
            // Lengths in pixels ("specify by length"); the percentage mode, the targets and the
            // ratios stay in the .sut.
            let v = s.variant;
            let t = &mut w.brush.taper;
            let len = |on: &str, length: &str| if flag(v, on) == Some(true) { num_in(v, length, 0.0, 10_000.0) } else { flag(v, on).map(|_| 0.0) };
            if let Some(l) = len(k::BRUSH_USE_IN, k::BRUSH_IN_LENGTH) {
                t.start = l;
            }
            if let Some(l) = len(k::BRUSH_USE_OUT, k::BRUSH_OUT_LENGTH) {
                t.end = l;
            }
            if let Some(on) = flag(v, k::BRUSH_IN_OUT_BY_SPEED) {
                t.by_speed = on;
            }
        },
        write: |w, v, _| {
            let t = &w.brush.taper;
            for (on, length, value) in [(k::BRUSH_USE_IN, k::BRUSH_IN_LENGTH, t.start_len()), (k::BRUSH_USE_OUT, k::BRUSH_OUT_LENGTH, t.end_len())] {
                put_flag(v, on, value > 0.0);
                if value > 0.0 {
                    put_num(v, length, f64::from(value), 1e-3);
                }
            }
            put_flag(v, k::BRUSH_IN_OUT_BY_SPEED, t.by_speed);
        },
        key: |w| json!([w.brush.taper.start_len(), w.brush.taper.end_len(), w.brush.taper.by_speed]),
    },
];
