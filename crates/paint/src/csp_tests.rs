//! Tests of the brush additions that map Clip Studio Paint settings (Clippy M13a): each one's
//! behaviour, its default leaving the old behaviour, and hostile values not crashing. The
//! bit-for-bit check of all defaults together is `golden_tests`.

use photocraft_color::PixelFormat;
use photocraft_geom::{Point, Rect};
use photocraft_raster::Surface;

use crate::brush::*;
use crate::dynamics::{DabGenerator, SpeedMeter, StepInput, VELOCITY_FULL_SPEED, smooth_points};
use crate::render::{BrushContext, StrokeRenderer, render_stroke};
use crate::{Dab, StrokePoint};

fn brush() -> BrushSettings {
    BrushSettings { pressure_size: false, spacing: 0.25, size: 20.0, ..Default::default() }
}

/// A straight stroke along y = 50 from x = 10 to 210, sampled every 2 px: `ms_per_px` ms per
/// pixel of travel (0 = no timestamps).
fn timed_line(ms_per_px: f64) -> Vec<StrokePoint> {
    (0..=100).map(|i| StrokePoint { time: f64::from(i) * 2.0 * ms_per_px, ..StrokePoint::new(10.0 + f64::from(i) * 2.0, 50.0, 1.0) }).collect()
}

fn all_dabs(b: &BrushSettings, pts: &[StrokePoint], zoom: f32) -> Vec<Dab> {
    let mut g = DabGenerator::new(b, zoom);
    let (mut out, mut dual) = (Vec::new(), Vec::new());
    g.push(pts, &mut out, &mut dual);
    g.finish(&mut out, &mut dual);
    out
}

fn mean_radius(d: &[Dab]) -> f32 {
    d.iter().map(|d| d.radius).sum::<f32>() / d.len().max(1) as f32
}

fn render(b: &BrushSettings, pts: &[StrokePoint]) -> Surface {
    let mut s = Surface::new(PixelFormat::RGBA32F);
    render_stroke(&mut s, b, pts, None, false, 1.0);
    s
}

fn step(speed: Option<f32>) -> StepInput {
    StepInput { point: StrokePoint::new(0.0, 0.0, 0.5), direction: 0.0, initial_direction: 0.0, step: 3, speed, distance: 0.0, end_taper: 1.0 }
}

fn json(b: &BrushSettings) -> serde_json::Value {
    serde_json::to_value(b).unwrap()
}

// ---------- velocity ----------

#[test]
fn the_speed_meter_measures_screen_pixels_per_millisecond() {
    let mut m = SpeedMeter::new(2.0);
    assert_eq!(m.push(&StrokePoint::new(0.0, 0.0, 1.0)), None, "one point has no speed");
    let p = |x: f64, t: f64| StrokePoint { time: t, ..StrokePoint::new(x, 0.0, 1.0) };
    let mut m = SpeedMeter::new(2.0);
    m.push(&p(0.0, 0.0));
    // 10 document px in 10 ms at 200 %: 2 screen px per ms.
    assert!((m.push(&p(10.0, 10.0)).unwrap() - 2.0).abs() < 1e-6);
    // Points without a later time add their distance to the next timed segment.
    let mut m = SpeedMeter::new(1.0);
    m.push(&p(0.0, 0.0));
    assert_eq!(m.push(&p(5.0, 0.0)), None);
    assert!((m.push(&p(10.0, 5.0)).unwrap() - 2.0).abs() < 1e-6);
    // It averages: a sudden stop doesn't drop to zero at once.
    let after = m.push(&p(10.0, 6.0)).unwrap();
    assert!(after > 0.5 && after < 2.0, "{after}");
    // Time going back restarts the measurement; hostile values never give a non-finite speed.
    for bad in [p(f64::NAN, 7.0), p(20.0, -5.0), p(f64::INFINITY, 9.0), p(1e300, 1e-300), p(30.0, f64::NAN)] {
        if let Some(v) = m.push(&bad) {
            assert!(v.is_finite() && v >= 0.0, "{bad:?} gave {v}");
        }
    }
    let mut m = SpeedMeter::new(f32::NAN);
    m.push(&p(0.0, 0.0));
    assert!(m.push(&p(3.0, 1.0)).is_some_and(f32::is_finite), "a bad zoom reads as 100 %");
}

#[test]
fn velocity_thins_a_fast_stroke_and_reads_full_without_timestamps() {
    let b = BrushSettings {
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic { minimum: 0.2, ..Dynamic::controlled(Control::Velocity) }, ..Default::default() },
        ..brush()
    };
    let slow = mean_radius(&all_dabs(&b, &timed_line(10.0), 1.0));
    let fast = mean_radius(&all_dabs(&b, &timed_line(0.05), 1.0));
    let untimed = all_dabs(&b, &timed_line(0.0), 1.0);
    assert!(slow > 9.0, "slow strokes paint near full size: {slow}");
    assert!((2.0 - 1e-4..3.0).contains(&fast), "fast strokes shrink to the minimum: {fast}");
    assert!(untimed.iter().all(|d| d.radius == 10.0), "no timestamps: full size");
    // Zoomed in, the same document speed is a faster hand: thinner.
    let medium = timed_line(1.0 / f64::from(VELOCITY_FULL_SPEED));
    assert!(mean_radius(&all_dabs(&b, &medium, 4.0)) < mean_radius(&all_dabs(&b, &medium, 0.25)));
    // As an opacity control.
    let o = BrushSettings { transfer: Transfer { enabled: true, opacity: Dynamic::controlled(Control::Velocity), ..Default::default() }, ..brush() };
    let fast = all_dabs(&o, &timed_line(0.05), 1.0);
    assert!(fast.iter().skip(10).all(|d| d.opacity < 0.05), "fast strokes are faint");
    // Its curve and its angle: the angle turns with the value, a fast stroke reads 0°.
    let mut d = Dynamic::controlled(Control::Velocity);
    d.curve = crate::ResponseCurve::new(&[[0.0, 0.0], [0.5, 0.9], [1.0, 1.0]]);
    let s = step(Some(VELOCITY_FULL_SPEED / 2.0));
    assert!((crate::dynamics::dynamic_value(&d, &s) - 0.9).abs() < 1e-3);
    assert_eq!(crate::dynamics::control_angle(Control::Velocity, 25, &step(Some(1e9))), 0.0);
    assert_eq!(crate::dynamics::control_angle(Control::Velocity, 25, &step(None)), 360.0);
}

#[test]
fn velocity_round_trips_and_chunking_does_not_change_it() {
    let d: Dynamic = serde_json::from_str(r#"{"control":"velocity"}"#).unwrap();
    assert_eq!(d.control, Control::Velocity);
    assert_eq!(serde_json::to_value(d).unwrap()["control"], "velocity");
    let b = BrushSettings { shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::controlled(Control::Velocity), ..Default::default() }, ..brush() };
    let pts: Vec<StrokePoint> =
        (0..60).map(|i| StrokePoint { time: f64::from(i * i) * 0.3, ..StrokePoint::new(10.0 + f64::from(i) * 3.0, 40.0 + f64::from(i % 5), 1.0) }).collect();
    let whole = all_dabs(&b, &pts, 1.0);
    let mut g = DabGenerator::new(&b, 1.0);
    let (mut out, mut dual) = (Vec::new(), Vec::new());
    for c in pts.chunks(7) {
        g.push(c, &mut out, &mut dual);
    }
    g.finish(&mut out, &mut dual);
    assert_eq!(out, whole);
}

// ---------- several controls ----------

#[test]
fn further_controls_multiply_with_the_dynamics_own() {
    let p = |pressure: f32, tilt: f32| StrokePoint { tilt_x: tilt, ..StrokePoint::new(0.0, 0.0, pressure) };
    let mut size = Dynamic { minimum: 0.0, ..Dynamic::controlled(Control::PenPressure) };
    size.controls = Controls::new(&[Influence::new(Control::PenTilt, 0.5)]);
    let b = BrushSettings { shape_dynamics: ShapeDynamics { enabled: true, size, ..Default::default() }, ..brush() };
    for (pressure, tilt) in [(1.0, 0.0), (0.5, 0.0), (1.0, 90.0), (0.5, 45.0)] {
        let s = StepInput { point: p(pressure, tilt), ..step(None) };
        let want = 20.0 * pressure * (0.5 + 0.5 * (1.0 - tilt / 90.0));
        let got = crate::dynamics::base_diameter(&b, &s);
        assert!((got - want).abs() < 1e-4, "pressure {pressure}, tilt {tilt}: {got} vs {want}");
    }
    // A further control alone (the dynamic's own control off) works, also on transfer.
    let opacity =
        Dynamic { controls: Controls::new(&[Influence::new(Control::PenPressure, 0.0), Influence::new(Control::Velocity, 0.25)]), ..Default::default() };
    assert!(opacity.is_active());
    let b = BrushSettings { transfer: Transfer { enabled: true, opacity, ..Default::default() }, ..brush() };
    let pts: Vec<StrokePoint> = timed_line(0.05).into_iter().map(|q| StrokePoint { pressure: 0.5, ..q }).collect();
    let d = all_dabs(&b, &pts, 1.0);
    assert!(d.iter().skip(10).all(|d| (d.opacity - 0.5 * 0.25).abs() < 0.02), "{:?}", d.last().map(|d| d.opacity));
    // Maximum: Clip Studio Paint's tilt range.
    let size = Dynamic { controls: Controls::new(&[Influence { maximum: 0.6, ..Influence::new(Control::PenTilt, 0.2) }]), ..Default::default() };
    let b = BrushSettings { shape_dynamics: ShapeDynamics { enabled: true, size, ..Default::default() }, ..brush() };
    let upright = crate::dynamics::base_diameter(&b, &StepInput { point: p(1.0, 0.0), ..step(None) });
    assert!((upright - 20.0 * 0.6).abs() < 1e-4, "{upright}");
}

#[test]
fn further_controls_are_capped_and_hostile_values_stay_finite() {
    let list = r#"[{"control":"penPressure"},{"control":"off"},{"control":"penTilt"},{"control":"velocity"},{"control":"fade"}]"#;
    let c: Controls = serde_json::from_str(list).unwrap();
    let kinds: Vec<Control> = c.as_slice().iter().map(|i| i.control).collect();
    assert_eq!(kinds, [Control::PenPressure, Control::PenTilt, Control::Velocity], "off dropped, capped at {MAX_EXTRA_CONTROLS}");
    let mut full = c;
    assert!(!full.push(Influence::new(Control::Fade, 0.0)));
    let mut empty = Controls::EMPTY;
    assert!(!empty.push(Influence::default()), "off is never added");
    // Unknown fields and missing ones read as defaults; a hostile minimum stays finite.
    let mut d: Dynamic = serde_json::from_str(r#"{"controls":[{"control":"penPressure","minimum":1e30,"maximum":-5,"bogus":1}]}"#).unwrap();
    assert_eq!(d.controls.as_slice().len(), 1);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -3.0] {
        d.controls = Controls::new(&[Influence { minimum: bad, maximum: bad, ..Influence::new(Control::PenPressure, 0.0) }]);
        let f = crate::dynamics::extra_factor(&d, &step(Some(f32::NAN)));
        assert!(f.is_finite() && (0.0..=1.0).contains(&f), "{bad}: {f}");
    }
    assert!(serde_json::from_str::<Controls>(r#"{"control":"penTilt"}"#).is_err(), "not a list: an error, not a panic");
}

#[test]
fn a_dynamic_without_further_controls_serialises_as_before() {
    let v = serde_json::to_value(Dynamic::controlled(Control::PenPressure)).unwrap();
    assert!(v.get("controls").is_none(), "{v}");
    let d = Dynamic { controls: Controls::new(&[Influence::new(Control::Velocity, 0.3)]), ..Default::default() };
    let back: Dynamic = serde_json::from_value(serde_json::to_value(d).unwrap()).unwrap();
    assert_eq!(back, d);
}

// ---------- anti-aliasing steps ----------

/// Pixels a hard 21 px dab covers partially.
fn soft_pixels(level: Option<u8>) -> usize {
    let b = BrushSettings { hardness: 1.0, aa_level: level, ..brush() };
    let ctx = BrushContext::new(&b);
    let d = Dab::round(Point::new(30.3, 30.6), 10.5, 1.0);
    let r = ctx.dab_rect(&d, false);
    let mut v = Vec::new();
    ctx.rasterize(&d, false, r, &mut v);
    v.iter().filter(|&&c| c > 0.01 && c < 0.99).count()
}

#[test]
fn anti_aliasing_steps_widen_the_edge() {
    let (none, weak, medium, strong) = (soft_pixels(Some(0)), soft_pixels(Some(1)), soft_pixels(Some(2)), soft_pixels(Some(3)));
    assert_eq!(none, 0, "step 0 is aliased");
    assert!(weak < medium && medium < strong, "{weak} < {medium} < {strong}");
    assert_eq!(medium, soft_pixels(None), "medium is PhotoCraft's own edge");
    assert_eq!(soft_pixels(Some(250)), strong, "out-of-range steps read as strong");
    // The step overrides the Pencil switch both ways.
    assert!(!BrushSettings { aliased: true, aa_level: Some(2), ..brush() }.is_aliased());
    assert!(BrushSettings { aliased: false, aa_level: Some(0), ..brush() }.is_aliased());
    assert!(BrushSettings { aliased: true, ..brush() }.is_aliased());
}

#[test]
fn anti_aliasing_step_zero_paints_like_the_pencil_and_medium_like_no_step() {
    let pts = timed_line(0.0);
    for fmt in [PixelFormat::RGBA8, PixelFormat::RGBA32F] {
        let paint = |b: &BrushSettings| {
            let mut s = Surface::new(fmt);
            render_stroke(&mut s, b, &pts, None, false, 1.0);
            s.read_region(Rect::new(0, 30, 230, 70))
        };
        assert_eq!(paint(&BrushSettings { aa_level: Some(0), ..brush() }), paint(&BrushSettings { aliased: true, ..brush() }), "{fmt:?}");
        assert_eq!(paint(&BrushSettings { aa_level: Some(2), ..brush() }), paint(&brush()), "{fmt:?}");
    }
    // Small aliased tips step on the pixel grid either way.
    let one = |b: BrushSettings| all_dabs(&BrushSettings { size: 1.0, spacing: 0.1, ..b }, &timed_line(0.0), 1.0);
    assert_eq!(one(BrushSettings { aa_level: Some(0), ..brush() }), one(BrushSettings { aliased: true, ..brush() }));
}

#[test]
fn anti_aliasing_step_is_omitted_from_json_until_set() {
    assert!(json(&brush()).get("aaLevel").is_none());
    let b: BrushSettings = serde_json::from_value(json(&BrushSettings { aa_level: Some(3), ..brush() })).unwrap();
    assert_eq!(b.aa_level, Some(3));
    assert!(serde_json::from_str::<BrushSettings>(r#"{"aaLevel":-1}"#).is_err(), "a negative step is rejected, not a panic");
}

// ---------- start and end ----------

fn tapered(start: f32, end: f32) -> BrushSettings {
    BrushSettings { taper: Taper { start, end, ..Default::default() }, ..brush() }
}

#[test]
fn the_start_grows_and_the_end_shrinks_over_their_lengths() {
    let pts = timed_line(0.0);
    let d = all_dabs(&tapered(50.0, 40.0), &pts, 1.0);
    let at = |x: f64| d.iter().min_by(|a, b| (a.center.x - x).abs().total_cmp(&(b.center.x - x).abs())).map_or(0.0, |d| d.radius);
    assert!(at(10.0) <= 0.5 + 1e-6, "starts from nothing: {}", at(10.0));
    assert!((at(35.0) - 5.0).abs() < 0.3, "half way up at 25 px: {}", at(35.0));
    assert_eq!(at(100.0), 10.0, "full size between the tapers");
    assert!((at(190.0) - 5.0).abs() < 0.3, "half way down 20 px before the end: {}", at(190.0));
    assert!(at(210.0) <= 0.5 + 1e-6, "ends in nothing: {}", at(210.0));
    // The dabs come in stroke order and the untapered middle is untouched.
    assert!(d.windows(2).all(|w| w[0].index < w[1].index && w[0].center.x <= w[1].center.x));
    assert!(d.iter().filter(|d| (60.0..170.0).contains(&d.center.x)).all(|d| d.radius == 10.0));
}

#[test]
fn the_end_taper_shows_in_the_live_preview_and_chunking_does_not_change_it() {
    let b = tapered(0.0, 60.0);
    let pts = timed_line(0.5);
    let fmt = PixelFormat::RGBA32F;
    let pre = Surface::new(fmt);
    let mut done = pre.clone();
    render_stroke(&mut done, &b, &pts, None, false, 1.0);
    let mut r = StrokeRenderer::new(&b, Some(fmt), 1.0);
    let mut live = pre.clone();
    for c in pts.chunks(9) {
        r.push(c);
        r.composite(&pre, &mut live, None, false, false);
    }
    // While the pen is down the last 60 px wait; the preview of lifting it draws them tapered.
    assert_eq!(live.rgba(200, 50)[3], 0.0);
    let mut tail = r.tail_preview().expect("held dabs are a tail");
    tail.composite(&pre, &mut live, None, false, false);
    let region = Rect::new(0, 30, 230, 70);
    assert_eq!(live.read_region(region), done.read_region(region));
    assert!(done.rgba(205, 50)[3] < done.rgba(150, 50)[3] || done.rgba(205, 57)[3] < done.rgba(150, 57)[3]);
}

#[test]
fn taper_targets_opacity_and_flow_and_follows_speed() {
    let b = BrushSettings { taper: Taper { start: 50.0, size: false, opacity: true, flow: true, ..Default::default() }, ..brush() };
    let d = all_dabs(&b, &timed_line(0.0), 1.0);
    assert!(d.iter().all(|d| d.radius == 10.0), "size untouched");
    let first = d.first().map_or(1.0, |d| d.opacity.max(d.alpha));
    assert!(first < 0.01, "opacity and flow start at nothing: {first}");
    assert!(d.iter().filter(|d| d.center.x > 70.0).all(|d| d.opacity == 1.0 && d.alpha == 1.0));
    // By speed: a slow stroke barely tapers, a fast one (or one without timestamps) fully.
    let by = BrushSettings { taper: Taper { start: 50.0, by_speed: true, ..Default::default() }, ..brush() };
    let near_start =
        |pts: &[StrokePoint]| all_dabs(&by, pts, 1.0).iter().filter(|d| (20.0..30.0).contains(&d.center.x)).map(|d| d.radius).fold(0.0f32, f32::max);
    assert!(near_start(&timed_line(100.0)) > 9.0, "slow: short taper");
    assert!(near_start(&timed_line(0.01)) < 5.0, "fast: full taper");
    assert!(near_start(&timed_line(0.0)) < 5.0, "untimed: full taper");
}

#[test]
fn taper_defaults_are_inactive_and_hostile_lengths_do_not_crash() {
    assert!(!Taper::default().is_active());
    assert!(json(&brush()).get("taper").is_none());
    assert!(!Taper { start: 30.0, size: false, ..Default::default() }.is_active(), "nothing to taper");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -10.0, 1e30] {
        let b = tapered(bad, bad);
        let d = all_dabs(&b, &timed_line(0.3), 1.0);
        assert!(!d.is_empty() && d.iter().all(|d| d.radius.is_finite() && d.radius >= 0.5), "{bad}");
        render(&b, &timed_line(0.3));
    }
    // An airbrush held still under a long end taper holds a bounded number of steps.
    let air = BrushSettings { build_up: true, build_up_rate: 1000.0, ..tapered(0.0, 5000.0) };
    let still: Vec<StrokePoint> = (0..2000).map(|i| StrokePoint { time: f64::from(i) * 10.0, ..StrokePoint::new(50.0, 50.0, 1.0) }).collect();
    let mut g = DabGenerator::new(&air, 1.0);
    let (mut out, mut dual) = (Vec::new(), Vec::new());
    g.push(&still, &mut out, &mut dual);
    assert!(!out.is_empty(), "the oldest held steps are painted once the cap is reached");
    g.finish(&mut out, &mut dual);
    assert!(out.len() > 19_000, "{}", out.len());
}

// ---------- stabilisation by speed, stickiness ----------

/// A zig-zag between y = 0 and 6, `dx` px and `ms` ms per point.
fn zigzag(dx: f64, ms: f64) -> Vec<StrokePoint> {
    (0..60).map(|i| StrokePoint { time: f64::from(i) * ms, ..StrokePoint::new(f64::from(i) * dx, if i % 2 == 0 { 0.0 } else { 6.0 }, 1.0) }).collect()
}

fn wobble(pts: &[StrokePoint]) -> f64 {
    pts.iter().skip(5).map(|p| (p.y - 3.0).abs()).sum::<f64>() / pts.len() as f64
}

#[test]
fn smoothing_by_speed_strengthens_slow_and_reduces_fast_strokes() {
    let base = Smoothing { amount: 0.5, ..Default::default() };
    // 0.25 and 5 screen px per ms.
    let slow = zigzag(4.0, 16.0);
    let fast = zigzag(40.0, 8.0);
    let w = |cfg: &Smoothing, pts: &[StrokePoint]| wobble(&smooth_points(cfg, 1.0, pts));
    let strengthen = Smoothing { by_speed: Some(SpeedSmoothing::StrengthenSlow), ..base.clone() };
    let reduce = Smoothing { by_speed: Some(SpeedSmoothing::ReduceFast), ..base.clone() };
    assert!(w(&strengthen, &slow) < w(&base, &slow), "slow strokes smoothed more");
    assert!(w(&reduce, &fast) > w(&base, &fast), "fast strokes smoothed less");
    // Without timestamps the amount stays as set.
    let untimed = zigzag(4.0, 0.0);
    assert_eq!(smooth_points(&strengthen, 1.0, &untimed), smooth_points(&base, 1.0, &untimed));
    assert_eq!(smooth_points(&reduce, 1.0, &untimed), smooth_points(&base, 1.0, &untimed));
    // Off by default and not written.
    assert!(serde_json::to_value(&base).unwrap().get("bySpeed").is_none());
    let back: Smoothing = serde_json::from_value(serde_json::to_value(&reduce).unwrap()).unwrap();
    assert_eq!(back, reduce);
}

#[test]
fn stickiness_ignores_small_wobbles_and_catches_up_at_the_end() {
    let jitter: Vec<StrokePoint> = (0..20).map(|i| StrokePoint::new(if i % 2 == 0 { 0.0 } else { 5.0 }, 0.0, 1.0)).collect();
    // Five steps of 2 px: wobbles of 5 px move nothing (but the end catches up).
    let sticky = Smoothing { stickiness: 5, catch_up_on_end: false, ..Default::default() };
    assert_eq!(smooth_points(&sticky, 1.0, &jitter).len(), 1);
    let mut cfg = Smoothing { stickiness: 5, ..Default::default() };
    let out = smooth_points(&cfg, 1.0, &[StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(100.0, 0.0, 1.0)]);
    assert_eq!(out.last().map(|p| p.x), Some(100.0));
    assert!((out[1].x - 90.0).abs() < 1e-9, "the brush stays 10 px behind: {}", out[1].x);
    // Zoomed in it stays fewer image pixels behind; more than five steps read as five.
    cfg.catch_up_on_end = false;
    let z = smooth_points(&cfg, 2.0, &[StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(100.0, 0.0, 1.0)]);
    assert!((z[1].x - 95.0).abs() < 1e-9);
    cfg.stickiness = 200;
    let capped = smooth_points(&cfg, 1.0, &[StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(100.0, 0.0, 1.0)]);
    assert!((capped[1].x - 90.0).abs() < 1e-9);
    // A held pointer doesn't keep the stroke asking for time once the brush has settled.
    let mut s = crate::dynamics::Smoother::new(&Smoothing { stickiness: 3, ..Default::default() }, 1.0);
    let mut out = Vec::new();
    s.push(StrokePoint::new(0.0, 0.0, 1.0), &mut out);
    s.push(StrokePoint { time: 16.0, ..StrokePoint::new(50.0, 0.0, 1.0) }, &mut out);
    assert!(!s.lagging());
    assert!(serde_json::to_value(Smoothing::default()).unwrap().get("stickiness").is_none());
}

// ---------- size and the view, at least one pixel ----------

#[test]
fn size_follows_the_view_zoom_when_asked() {
    let linked = BrushSettings { size_follows_view: true, ..brush() };
    let line = timed_line(0.0);
    assert!(all_dabs(&linked, &line, 2.0).iter().all(|d| d.radius == 5.0), "20 screen px at 200 % = 10 document px");
    assert!(all_dabs(&linked, &line, 0.5).iter().all(|d| d.radius == 20.0));
    assert_eq!(all_dabs(&brush(), &line, 2.0), all_dabs(&brush(), &line, 1.0), "off: the zoom only matters to smoothing");
    for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(all_dabs(&linked, &line, zoom).iter().all(|d| d.radius == 10.0), "a bad zoom ({zoom}) leaves the size");
    }
    // Tiny zooms stay within the renderer's bounds.
    assert!(all_dabs(&BrushSettings { size: 4000.0, ..linked.clone() }, &line[..3], 1e-6).iter().all(|d| d.radius <= MAX_BRUSH_SIZE / 2.0));
    assert!(json(&brush()).get("sizeFollowsView").is_none());
}

#[test]
fn without_at_least_one_pixel_thin_dabs_fade() {
    // 2 px at 20 % pressure: 0.4 px.
    let light: Vec<StrokePoint> = timed_line(0.0).into_iter().map(|p| StrokePoint { pressure: 0.2, ..p }).collect();
    let thin = BrushSettings { size: 2.0, pressure_size: true, ..brush() };
    assert!(all_dabs(&thin, &light, 1.0).iter().all(|d| d.alpha == 1.0 && d.radius == 0.5), "on: as before");
    let fading = BrushSettings { min_one_px: false, ..thin };
    let d = all_dabs(&fading, &light, 1.0);
    assert!(d.iter().all(|d| (d.alpha - 0.4).abs() < 1e-6 && d.radius == 0.5), "{:?}", d.first());
    // Thicker dabs are untouched.
    assert!(all_dabs(&BrushSettings { min_one_px: false, ..brush() }, &timed_line(0.0), 1.0).iter().all(|d| d.alpha == 1.0));
    // JSON: only "off" is written, and old brushes read as "on".
    assert!(json(&brush()).get("minOnePx").is_none());
    assert_eq!(json(&fading)["minOnePx"], false);
    assert!(serde_json::from_str::<BrushSettings>("{}").unwrap().min_one_px);
}

// ---------- darken, texture turn, dual texture, wet edge ----------

/// Alpha where a stroke that runs back over itself crosses x = 60.
fn retraced_alpha(b: &BrushSettings) -> f32 {
    let pts = [StrokePoint::new(20.0, 50.0, 1.0), StrokePoint::new(100.0, 50.0, 1.0), StrokePoint::new(20.0, 50.0, 1.0)];
    render(b, &pts).rgba(60, 50)[3]
}

#[test]
fn darken_keeps_overlapping_dabs_from_building_up() {
    let soft = BrushSettings { flow: 0.3, ..brush() };
    let built = retraced_alpha(&soft);
    let darkened = retraced_alpha(&BrushSettings { blend_darken: true, ..soft.clone() });
    assert!(built > 0.9, "flow builds up: {built}");
    assert!((darkened - 0.3).abs() < 0.01, "darken keeps one dab's coverage: {darkened}");
    // Under the opacity ceiling too.
    let capped = retraced_alpha(&BrushSettings {
        blend_darken: true,
        transfer: Transfer { enabled: true, opacity: Dynamic::controlled(Control::PenPressure), ..Default::default() },
        ..soft
    });
    assert!(capped <= 0.3 + 1e-4);
    assert!(json(&brush()).get("blendDarken").is_none());
}

fn stripes() -> crate::tile::GrayTile {
    crate::tile::GrayTile::from_fn(8, 8, |x, _| if x < 4 { 0.0 } else { 1.0 })
}

#[test]
fn the_texture_turns_and_an_unturned_one_samples_as_before() {
    let tex = |rotation: f32| {
        let b = BrushSettings { texture: Texture { enabled: true, pattern: Pattern::Tile(stripes()), rotation, ..Default::default() }, ..brush() };
        let ctx = BrushContext::new(&b);
        (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).map(|(x, y)| ctx.texture_at(x, y)).collect::<Vec<f32>>()
    };
    let column = |v: &[f32], x: usize| (0..16).map(|y| v[y * 16 + x]).collect::<Vec<f32>>();
    let row = |v: &[f32], y: usize| v[y * 16..y * 16 + 16].to_vec();
    let flat = tex(0.0);
    assert!(column(&flat, 1).iter().all(|&t| t == 0.0) && column(&flat, 5).iter().all(|&t| t == 1.0), "vertical stripes");
    let turned = tex(90.0);
    let varies = |v: Vec<f32>| v.iter().any(|&t| t < 0.5) && v.iter().any(|&t| t > 0.5);
    assert!(!varies(row(&turned, 1)) && varies(column(&turned, 1)), "a quarter turn makes them horizontal");
    assert_eq!(tex(360.0), flat, "a full turn is no turn");
    assert_eq!(tex(f32::NAN), flat, "NaN is no turn");
    assert!(tex(f32::INFINITY).iter().all(|t| t.is_finite()));
    assert!(serde_json::to_value(Texture::default()).unwrap().get("rotation").is_none());
}

#[test]
fn the_dual_brush_texture_masks_the_second_tip() {
    let dual = DualBrush { enabled: true, size: 30.0, ..Default::default() };
    let plain = BrushSettings { dual_brush: dual.clone(), ..brush() };
    let black = Texture { enabled: true, pattern: Pattern::Tile(crate::tile::GrayTile::from_fn(4, 4, |_, _| 0.0)), ..Default::default() };
    let masked = BrushSettings { dual_brush: DualBrush { texture: black, ..dual.clone() }, ..brush() };
    let line = timed_line(0.0);
    assert!(render(&plain, &line).rgba(100, 50)[3] > 0.9);
    assert_eq!(render(&masked, &line).rgba(100, 50)[3], 0.0, "a black texture on the mask removes the paint");
    // A disabled dual texture changes nothing and isn't written.
    let off = BrushSettings { dual_brush: DualBrush { texture: Texture { enabled: false, rotation: 20.0, ..Default::default() }, ..dual }, ..brush() };
    let region = Rect::new(0, 30, 230, 70);
    assert_eq!(render(&off, &line).read_region(region), render(&plain, &line).read_region(region));
    assert!(json(&plain)["dualBrush"].get("texture").is_none());
}

#[test]
fn the_wet_edge_shape_sets_interior_and_rim() {
    let wet = |edge: WetEdge| {
        let b = BrushSettings { size: 40.0, hardness: 1.0, wet_edges: true, wet_edge: edge, ..brush() };
        let s = render(&b, &[StrokePoint::new(50.0, 50.0, 1.0)]);
        (s.rgba(50, 50)[3], s.rgba(50 + 18, 50)[3])
    };
    let (centre, rim) = wet(WetEdge::default());
    assert!((centre - 0.5).abs() < 0.01 && rim > 0.8, "{centre} {rim}");
    let (centre, _) = wet(WetEdge { interior: 0.2, width: 0.5 });
    assert!((centre - 0.2).abs() < 0.01, "a lighter interior: {centre}");
    let (centre, rim) = wet(WetEdge { interior: 1.0, width: 0.5 });
    assert!(centre > 0.99 && rim > 0.99, "interior 1: no wet look");
    for bad in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
        let (c, r) = wet(WetEdge { interior: bad, width: bad });
        assert!(c.is_finite() && r.is_finite(), "{bad}");
    }
    assert!(json(&brush()).get("wetEdge").is_none());
}
