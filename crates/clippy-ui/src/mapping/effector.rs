//! Clip Studio Paint's pen settings (an `…Effector` blob) ↔ a PhotoCraft brush [`Dynamic`].
//!
//! | Effector | Dynamic |
//! |---|---|
//! | first active of pressure, tilt, velocity | `control`, `minimum` (its minimum %), `curve` (its curve) |
//! | the other active ones | `controls` (an [`Influence`] each, same minimum and curve) |
//! | random, minimum m % | `jitter` = 1 − m/100 (the value drops at random down to m %) |
//!
//! Velocity has no curve in the file (linear). A PhotoCraft control Clip Studio Paint has no
//! counterpart for (fade, wheel, rotation, direction) is left out when writing.

use photocraft_clippy_sut::{Curve, Effector, Source, Sources};
use photocraft_engine::paint::brush::{Control, Controls, Dynamic, Influence};
use photocraft_engine::paint::curve::ResponseCurve;
use serde_json::{Value, json};

/// The sources a [`Dynamic`] can stand for, in the order the effector's slots list them.
const SOURCES: [(Source, Control); 3] = [(Source::Pressure, Control::PenPressure), (Source::Tilt, Control::PenTilt), (Source::Velocity, Control::Velocity)];

fn control_of(source: Source) -> Option<Control> {
    SOURCES.iter().find(|(s, _)| *s == source).map(|(_, c)| *c)
}

fn source_of(control: Control) -> Option<Source> {
    SOURCES.iter().find(|(_, c)| *c == control).map(|(s, _)| *s)
}

/// A minimum percentage as a 0..1 fraction.
fn fraction(percent: i32) -> f32 {
    (percent.clamp(0, 100) as f32) / 100.0
}

fn percent(fraction: f32) -> i32 {
    if fraction.is_nan() { 0 } else { (fraction.clamp(0.0, 1.0) * 100.0).round() as i32 }
}

/// A curve from the file; linear when it is missing, opaque or has fewer than two points.
fn curve_in(curve: Option<&Curve>) -> ResponseCurve {
    match curve {
        Some(c) if c.opaque.is_none() && c.points.len() >= 2 => {
            let pts: Vec<[f32; 2]> = c.points.iter().map(|&(x, y)| [x as f32, y as f32]).collect();
            ResponseCurve::new(&pts)
        }
        _ => ResponseCurve::LINEAR,
    }
}

fn curve_out(curve: &ResponseCurve) -> Curve {
    Curve::new(curve.points().iter().map(|p| (f64::from(p[0]), f64::from(p[1]))).collect())
}

/// The dynamic a pen setting stands for.
pub fn to_dynamic(e: &Effector) -> Dynamic {
    let active = e.active();
    let mut d = Dynamic::default();
    let mut extra = Vec::new();
    for source in [Source::Pressure, Source::Tilt, Source::Velocity] {
        if !active.contains(source) {
            continue;
        }
        let Some(control) = control_of(source) else { continue };
        let minimum = fraction(e.minimum(source));
        let curve = match source {
            Source::Pressure => curve_in(e.pressure_curve()),
            Source::Tilt => curve_in(e.tilt_curve()),
            _ => ResponseCurve::LINEAR,
        };
        if d.control == Control::Off {
            d.control = control;
            d.minimum = minimum;
            d.curve = curve;
        } else {
            extra.push(Influence { control, minimum, maximum: 1.0, curve });
        }
    }
    d.controls = Controls::new(&extra);
    if active.contains(Source::Random) {
        d.jitter = 1.0 - fraction(e.minimum(Source::Random));
    }
    d
}

/// Write `d` into `e`: the active sources, their minimums and curves. Other slots (offered
/// sources, the tilt range, unknown bits) stay as they are.
pub fn write_dynamic(d: &Dynamic, e: &mut Effector) {
    let mut active = Sources(e.active().0 & !(Source::Pressure.bit() | Source::Tilt.bit() | Source::Velocity.bit() | Source::Random.bit()));
    let mut pressure_curve = None;
    let mut tilt_curve = None;
    let own = Influence { control: d.control, minimum: d.minimum, maximum: 1.0, curve: d.curve };
    for i in std::iter::once(&own).chain(d.controls.as_slice()) {
        let Some(source) = source_of(i.control) else { continue };
        if active.contains(source) {
            continue;
        }
        active = active.with(source, true);
        e.set_minimum(source, percent(i.minimum));
        match source {
            Source::Pressure => pressure_curve = Some(curve_out(&i.curve)),
            Source::Tilt => tilt_curve = Some(curve_out(&i.curve)),
            _ => {}
        }
    }
    if d.jitter > 0.0 {
        active = active.with(Source::Random, true);
        e.set_minimum(Source::Random, percent(1.0 - d.jitter));
    }
    // Keep the stored curves of inactive sources and curves that read the same (Clip Studio
    // Paint's own points, which PhotoCraft may hold only approximately).
    if let Some(c) = pressure_curve
        && curve_in(e.pressure_curve()) != curve_in(Some(&c))
    {
        e.set_pressure_curve(Some(c));
    }
    if let Some(c) = tilt_curve
        && curve_in(e.tilt_curve()) != curve_in(Some(&c))
    {
        e.set_tilt_curve(Some(c));
    }
    e.set_active(active);
}

/// Whether two dynamics read the same as far as an effector can say (what [`to_dynamic`] gives
/// back), so an unchanged setting isn't rewritten.
pub fn same(a: &Dynamic, b: &Dynamic) -> bool {
    key(a) == key(b)
}

/// The parts of a dynamic an effector holds, for change detection.
pub fn key(d: &Dynamic) -> Value {
    let inf = |i: &Influence| source_of(i.control).map(|_| json!([format!("{:?}", i.control), percent(i.minimum), i.curve.points()]));
    let own = Influence { control: d.control, minimum: d.minimum, maximum: 1.0, curve: d.curve };
    let list: Vec<Value> = std::iter::once(&own).chain(d.controls.as_slice()).filter_map(inf).collect();
    let jitter = if d.jitter > 0.0 { percent(1.0 - d.jitter) } else { -1 };
    json!([list, jitter])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effector(active: &[Source], minimums: &[(Source, i32)]) -> Effector {
        let mut e = Effector::new(Sources(0x1F0));
        e.set_active(active.iter().fold(Sources(0), |s, x| s.with(*x, true)));
        for (s, m) in minimums {
            e.set_minimum(*s, *m);
        }
        e
    }

    #[test]
    fn pressure_with_a_curve_becomes_the_control() {
        let mut e = effector(&[Source::Pressure], &[(Source::Pressure, 30)]);
        e.set_pressure_curve(Some(Curve::new(vec![(0.0, 0.0), (0.5, 0.2), (1.0, 1.0)])));
        let d = to_dynamic(&e);
        assert_eq!(d.control, Control::PenPressure);
        assert!((d.minimum - 0.3).abs() < 1e-6);
        assert!((d.curve.eval(0.5) - 0.2).abs() < 1e-4);
        assert_eq!(d.jitter, 0.0);
        assert!(d.controls.is_empty());
    }

    #[test]
    fn several_sources_and_random() {
        let e = effector(&[Source::Pressure, Source::Tilt, Source::Velocity, Source::Random], &[(Source::Tilt, 50), (Source::Random, 20)]);
        let d = to_dynamic(&e);
        assert_eq!(d.control, Control::PenPressure);
        let extra: Vec<_> = d.controls.as_slice().iter().map(|i| i.control).collect();
        assert_eq!(extra, [Control::PenTilt, Control::Velocity]);
        assert!((d.controls.as_slice()[0].minimum - 0.5).abs() < 1e-6);
        assert!((d.jitter - 0.8).abs() < 1e-6);
    }

    #[test]
    fn writing_back_an_unchanged_dynamic_keeps_the_bytes() {
        let mut e = effector(&[Source::Pressure, Source::Random], &[(Source::Pressure, 12), (Source::Random, 40)]);
        e.set_pressure_curve(Some(Curve::new(vec![(0.0, 0.1), (0.31, 0.27), (0.77, 0.81), (1.0, 1.0)])));
        let bytes = e.encode();
        let mut back = Effector::decode(&bytes).unwrap();
        write_dynamic(&to_dynamic(&e), &mut back);
        assert_eq!(back.encode(), bytes);
    }

    #[test]
    fn a_changed_dynamic_writes_sources_minimums_and_curves() {
        let mut e = effector(&[], &[]);
        let d = Dynamic {
            control: Control::PenTilt,
            minimum: 0.25,
            curve: ResponseCurve::new(&[[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]]),
            controls: Controls::new(&[Influence::new(Control::Velocity, 0.4), Influence::new(Control::Fade, 0.1)]),
            jitter: 0.5,
            ..Dynamic::default()
        };
        write_dynamic(&d, &mut e);
        let back = to_dynamic(&Effector::decode(&e.encode()).unwrap());
        assert!(same(&back, &d), "{back:?}");
        assert!(e.active().contains(Source::Tilt) && e.active().contains(Source::Velocity) && e.active().contains(Source::Random));
        assert!(!e.active().contains(Source::Pressure));
        assert_eq!(e.minimum(Source::Random), 50);
        // Fade has no counterpart: left out.
        assert_eq!(back.controls.as_slice().len(), 1);
    }

    #[test]
    fn unknown_bits_survive_a_write() {
        let mut e = Effector::new(Sources(0x1F0));
        e.set_active(Sources(0x100 | 0x10));
        write_dynamic(&Dynamic::controlled(Control::Velocity), &mut e);
        assert_eq!(e.active().0, 0x100 | 0x40);
    }
}
