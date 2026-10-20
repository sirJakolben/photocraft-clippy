//! A digest of the dabs and pixels of strokes through every brush section, recorded on the engine
//! before the Clip Studio Paint additions (Clippy M13a, base `356ecf18`). Every addition defaults
//! to the old behaviour, so the digest must not move: a change here means an old brush, preset or
//! recorded stroke paints differently.
//!
//! Uses only the API the base had, so the same file runs there to re-record the digest.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::brush::*;
use crate::curve::ResponseCurve;
use crate::render::StrokeRenderer;
use crate::tile::GrayTile;
use crate::{StrokePoint, presets};

/// The digest recorded on the base engine.
const GOLDEN: u64 = 0x0075_746b_87b1_dafc;

/// FNV-1a over 64-bit words.
struct Digest(u64);

impl Digest {
    fn word(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn f32(&mut self, v: f32) {
        self.word(u64::from(v.to_bits()));
    }
}

/// A wavy pen stroke with pressure, tilt, rotation and timestamps (`timed`) or without them.
fn pen_stroke(timed: bool) -> Vec<StrokePoint> {
    (0..40)
        .map(|i| {
            let t = f64::from(i) / 39.0;
            StrokePoint {
                x: 12.0 + t * 100.0,
                y: 48.0 + (t * 9.0).sin() * 25.0,
                pressure: 0.2 + 0.8 * (t * 5.0).sin().abs() as f32,
                tilt_x: (30.0 * (t * 3.0).cos()) as f32,
                tilt_y: (20.0 * (t * 2.0).sin()) as f32,
                rotation: (t * 300.0) as f32,
                wheel: 0.5,
                time: if timed { f64::from(i * i) * 1.5 } else { 0.0 },
            }
        })
        .collect()
}

fn brushes() -> Vec<BrushSettings> {
    let base = BrushSettings { size: 14.0, seed: 7, ..Default::default() };
    let tip = GrayTile::from_fn(9, 6, |x, y| ((x * 3 + y * 5) % 11) as f32 / 10.0);
    let curve = ResponseCurve::new(&[[0.0, 0.0], [0.4, 0.7], [1.0, 1.0]]);
    let mut v = vec![
        base.clone(),
        BrushSettings { pressure_size: false, pressure_opacity: true, hardness: 0.3, ..base.clone() },
        BrushSettings {
            shape_dynamics: ShapeDynamics {
                enabled: true,
                size: Dynamic { minimum: 0.2, curve, ..Dynamic::controlled(Control::PenPressure) },
                angle: Dynamic { jitter: 0.3, ..Dynamic::controlled(Control::Direction) },
                roundness: Dynamic { minimum: 0.3, ..Dynamic::controlled(Control::PenTilt) },
                tilt_scale: 1.0,
                flip_x_jitter: true,
                brush_projection: true,
                ..Default::default()
            },
            roundness: 0.6,
            ..base.clone()
        },
        BrushSettings {
            shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic { fade_steps: 9, ..Dynamic::controlled(Control::Fade) }, ..Default::default() },
            scattering: Scattering { enabled: true, scatter: Dynamic::jitter(1.5), count: 3, count_jitter: Dynamic::jitter(0.5), ..Default::default() },
            ..base.clone()
        },
        BrushSettings {
            texture: Texture { enabled: true, each_tip: true, depth_jitter: Dynamic::jitter(0.6), mode: MaskMode::Height, ..Default::default() },
            dual_brush: DualBrush { enabled: true, size: 9.0, scatter: 0.5, ..Default::default() },
            ..base.clone()
        },
        BrushSettings {
            color_dynamics: ColorDynamics { enabled: true, fg_bg: Dynamic::controlled(Control::StylusWheel), hue_jitter: 0.2, ..Default::default() },
            transfer: Transfer { enabled: true, opacity: Dynamic::controlled(Control::PenPressure), flow: Dynamic::jitter(0.4), ..Default::default() },
            color: [0.8, 0.2, 0.1, 1.0],
            ..base.clone()
        },
        BrushSettings { wet_edges: true, noise: true, hardness: 0.0, ..base.clone() },
        BrushSettings { aliased: true, size: 1.0, ..base.clone() },
        BrushSettings { aliased: true, square: true, size: 5.0, ..base.clone() },
        BrushSettings { build_up: true, build_up_rate: 50.0, spacing_enabled: false, ..base.clone() },
        BrushSettings { smoothing: Smoothing { amount: 0.6, ..Default::default() }, ..base.clone() },
        BrushSettings { smoothing: Smoothing { amount: 0.3, pulled_string: true, ..Default::default() }, ..base.clone() },
        BrushSettings { tip: TipShape::Sampled(tip), angle: 30.0, flip_y: true, ..base.clone() },
        BrushSettings { mode: photocraft_color::BlendMode::Multiply, opacity: 0.6, flow: 0.5, ..base.clone() },
        BrushSettings { erase: false, mode: photocraft_color::BlendMode::Dissolve, ..base },
    ];
    v.extend(presets::builtin().into_iter().map(|p| BrushSettings { size: p.brush.size.min(30.0), seed: 3, ..p.brush }));
    v
}

#[test]
fn default_fields_keep_every_stroke_bit_for_bit() {
    let mut h = Digest(0xcbf2_9ce4_8422_2325);
    for b in brushes() {
        for timed in [false, true] {
            let pts = pen_stroke(timed);
            for fmt in [PixelFormat::RGBA8, PixelFormat::RGBA32F] {
                let mut s = Surface::new(fmt);
                let pre = s.clone();
                let mut r = StrokeRenderer::new(&b, Some(fmt), 1.5).record_dabs();
                for chunk in pts.chunks(7) {
                    r.push(chunk);
                }
                r.finish();
                r.composite(&pre, &mut s, None, false, true);
                for d in r.dabs() {
                    for v in [d.center.x as f32, d.center.y as f32, d.radius, d.alpha, d.angle, d.roundness, d.opacity, d.depth, d.proj_scale, d.wet, d.mix] {
                        h.f32(v);
                    }
                    h.word(d.index);
                }
                for v in s.read_region(Rect::new(-20, 0, 140, 100)) {
                    h.f32(v);
                }
            }
        }
    }
    assert_eq!(h.0, GOLDEN, "digest {:#x}", h.0);
}
