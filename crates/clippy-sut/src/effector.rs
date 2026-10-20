//! `…Effector` blobs: how pen pressure, tilt, velocity and randomness change one setting
//! (Clip Studio Paint's pen settings, "Stifteinstellungen").
//!
//! Layout (all big-endian), checked against all 5 455 effector blobs of a Clip Studio Paint 1.5
//! tool database:
//!
//! | Offset | Field |
//! |---|---|
//! | 0 | `i32` header length in bytes (44; 40 in older rows, which lack slot 10) |
//! | 4 | `i32` sources the setting offers (bit mask, [`Source`]) |
//! | 8 | `i32` sources switched on |
//! | 12–24 | `i32` minimum value in percent for pressure, tilt, velocity, random |
//! | 28 | `i32` unknown, always 0 |
//! | 32 | `i32` byte length of the pressure curve (0 = none) |
//! | 36 | `i32` byte length of the tilt curve (0 = none) |
//! | 40 | `i32` tilt range (100–500; 500 when tilt is off) |
//! | header | pressure curve, then tilt curve |
//!
//! A curve is `i32` 12 (its header length), `i32` point count, `i32` 16 (point size), then the
//! points as two `f64` (input, output), both in 0–1. Slot meanings beyond the bit masks, the
//! pressure minimum and the curve lengths are inferred from the data (see the crate README).
//! Decoding keeps every slot and any trailing bytes, so [`Effector::encode`] reproduces the input.

use crate::bytes::{Reader, put_i32};
use crate::{Result, malformed};

const WHAT: &str = "effector";
const SLOT_AVAILABLE: usize = 1;
const SLOT_ACTIVE: usize = 2;
const SLOT_MIN_PRESSURE: usize = 3;
const SLOT_PRESSURE_CURVE: usize = 8;
const SLOT_TILT_CURVE: usize = 9;
const SLOT_TILT_RANGE: usize = 10;
const CURVE_HEADER: i32 = 12;
const CURVE_POINT: i32 = 16;
/// Header length Clip Studio Paint 1.5 writes.
pub const HEADER_LEN: usize = 44;
/// Largest header accepted; observed headers are 40 or 44 bytes.
const MAX_HEADER: usize = 4096;

/// An input that can drive a setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// Pen pressure (bit `0x10`).
    Pressure,
    /// Pen tilt (bit `0x20`).
    Tilt,
    /// Stroke velocity (bit `0x40`).
    Velocity,
    /// Random per dab (bit `0x80`).
    Random,
}

impl Source {
    /// All sources in slot order.
    pub const ALL: [Source; 4] = [Source::Pressure, Source::Tilt, Source::Velocity, Source::Random];

    /// The bit of this source in the masks.
    pub const fn bit(self) -> u32 {
        match self {
            Source::Pressure => 0x10,
            Source::Tilt => 0x20,
            Source::Velocity => 0x40,
            Source::Random => 0x80,
        }
    }

    const fn min_slot(self) -> usize {
        SLOT_MIN_PRESSURE
            + match self {
                Source::Pressure => 0,
                Source::Tilt => 1,
                Source::Velocity => 2,
                Source::Random => 3,
            }
    }
}

/// A bit mask of sources. Unknown bits (`0x100` appears in "offered" masks) are kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sources(pub u32);

impl Sources {
    /// Whether `source` is in the mask.
    pub const fn contains(self, source: Source) -> bool {
        self.0 & source.bit() != 0
    }

    /// The mask with `source` set or cleared.
    #[must_use]
    pub const fn with(self, source: Source, on: bool) -> Self {
        if on { Sources(self.0 | source.bit()) } else { Sources(self.0 & !source.bit()) }
    }

    /// The known sources in the mask.
    pub fn iter(self) -> impl Iterator<Item = Source> {
        Source::ALL.into_iter().filter(move |s| self.contains(*s))
    }
}

/// A response curve: points (input, output), both 0–1, sorted by input.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// The points.
    pub points: Vec<(f64, f64)>,
    /// The verbatim bytes of a curve whose layout is not the known one; `points` is then empty.
    pub opaque: Option<Vec<u8>>,
}

impl Curve {
    /// A curve through `points`.
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self { points, opaque: None }
    }

    /// The straight line (0,0)–(1,1) Clip Studio Paint writes for an untouched curve.
    pub fn linear() -> Self {
        Self::new(vec![(0.0, 0.0), (1.0, 1.0)])
    }

    fn decode(bytes: &[u8]) -> Self {
        Self::decode_known(bytes).unwrap_or_else(|| Self { points: Vec::new(), opaque: Some(bytes.to_vec()) })
    }

    fn decode_known(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader::new(bytes, WHAT);
        let (header, count, size) = (r.i32_be().ok()?, r.i32_be().ok()?, r.i32_be().ok()?);
        let count = usize::try_from(count).ok()?;
        if header != CURVE_HEADER || size != CURVE_POINT || count.checked_mul(16)? != r.remaining() {
            return None;
        }
        let mut points = Vec::with_capacity(count);
        for _ in 0..count {
            points.push((r.f64_be().ok()?, r.f64_be().ok()?));
        }
        Some(Self::new(points))
    }

    fn encode(&self, out: &mut Vec<u8>) {
        if let Some(raw) = &self.opaque {
            out.extend_from_slice(raw);
            return;
        }
        put_i32(out, CURVE_HEADER);
        put_i32(out, i32::try_from(self.points.len()).unwrap_or(i32::MAX));
        put_i32(out, CURVE_POINT);
        for (x, y) in &self.points {
            out.extend_from_slice(&x.to_be_bytes());
            out.extend_from_slice(&y.to_be_bytes());
        }
    }

    fn encoded_len(&self) -> usize {
        match &self.opaque {
            Some(raw) => raw.len(),
            None => 12usize.saturating_add(self.points.len().saturating_mul(16)),
        }
    }

    /// The output for `input` (0–1), interpolated linearly between points; `None` for an opaque or
    /// empty curve. Clip Studio Paint draws the curve smoothed; linear is close for M8's mapping.
    pub fn eval(&self, input: f64) -> Option<f64> {
        let first = self.points.first()?;
        if input <= first.0 {
            return Some(first.1);
        }
        for pair in self.points.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            if input <= x1 {
                let span = x1 - x0;
                return Some(if span > 0.0 { y0 + (y1 - y0) * (input - x0) / span } else { y1 });
            }
        }
        self.points.last().map(|p| p.1)
    }
}

/// A decoded `…Effector` blob.
#[derive(Clone, Debug, PartialEq)]
pub struct Effector {
    slots: Vec<i32>,
    pressure_curve: Option<Curve>,
    tilt_curve: Option<Curve>,
    trailing: Vec<u8>,
}

impl Effector {
    /// A Clip Studio Paint 1.5 effector offering `available` with nothing switched on.
    pub fn new(available: Sources) -> Self {
        let mut slots = vec![0; HEADER_LEN / 4];
        slots[0] = i32::try_from(HEADER_LEN).unwrap_or(44);
        slots[SLOT_AVAILABLE] = available.0 as i32;
        slots[SLOT_TILT_RANGE] = 500;
        Self { slots, pressure_curve: None, tilt_curve: None, trailing: Vec::new() }
    }

    /// Decodes an effector blob.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, WHAT);
        let header = r.i32_be()?;
        let header_len = usize::try_from(header).unwrap_or(0);
        if !(8..=MAX_HEADER).contains(&header_len) || header_len % 4 != 0 {
            return malformed(WHAT, format!("header length {header}"));
        }
        if header_len > bytes.len() {
            return malformed(WHAT, format!("header length {header} beyond the blob's {} bytes", bytes.len()));
        }
        let mut slots = vec![header];
        for _ in 1..header_len / 4 {
            slots.push(r.i32_be()?);
        }
        let curve_len = |slot: usize| -> Result<usize> {
            let len = slots.get(slot).copied().unwrap_or(0);
            usize::try_from(len).or_else(|_| malformed(WHAT, format!("curve length {len}")))
        };
        let (pressure_len, tilt_len) = (curve_len(SLOT_PRESSURE_CURVE)?, curve_len(SLOT_TILT_CURVE)?);
        let pressure_curve = (pressure_len > 0).then(|| r.take(pressure_len).map(Curve::decode)).transpose()?;
        let tilt_curve = (tilt_len > 0).then(|| r.take(tilt_len).map(Curve::decode)).transpose()?;
        let trailing = r.take(r.remaining())?.to_vec();
        Ok(Self { slots, pressure_curve, tilt_curve, trailing })
    }

    /// Encodes the effector; for a decoded blob without changes this gives the input bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut slots = self.slots.clone();
        if (self.pressure_curve.is_some() || self.tilt_curve.is_some()) && slots.len() <= SLOT_TILT_CURVE {
            // A header too short to hold the curve lengths grows to the 40-byte layout.
            slots.resize(SLOT_TILT_CURVE + 1, 0);
            if let Some(s) = slots.first_mut() {
                *s = 40;
            }
        }
        let len_of = |c: &Option<Curve>| c.as_ref().map_or(0, |c| i32::try_from(c.encoded_len()).unwrap_or(i32::MAX));
        if let Some(s) = slots.get_mut(SLOT_PRESSURE_CURVE) {
            *s = len_of(&self.pressure_curve);
        }
        if let Some(s) = slots.get_mut(SLOT_TILT_CURVE) {
            *s = len_of(&self.tilt_curve);
        }
        let mut out = Vec::with_capacity(slots.len() * 4);
        for s in &slots {
            put_i32(&mut out, *s);
        }
        for curve in [&self.pressure_curve, &self.tilt_curve].into_iter().flatten() {
            curve.encode(&mut out);
        }
        out.extend_from_slice(&self.trailing);
        out
    }

    /// Sources this setting offers.
    pub fn available(&self) -> Sources {
        Sources(self.slot(SLOT_AVAILABLE) as u32)
    }

    /// Sources switched on.
    pub fn active(&self) -> Sources {
        Sources(self.slot(SLOT_ACTIVE) as u32)
    }

    /// Switches sources on or off.
    pub fn set_active(&mut self, active: Sources) {
        self.set_slot(SLOT_ACTIVE, active.0 as i32);
    }

    /// Minimum value in percent (0–100) the setting keeps when `source` is at its lowest.
    pub fn minimum(&self, source: Source) -> i32 {
        self.slot(source.min_slot())
    }

    /// Sets the minimum for `source`.
    pub fn set_minimum(&mut self, source: Source, percent: i32) {
        self.set_slot(source.min_slot(), percent);
    }

    /// Tilt range slot (100–500); meaning inferred, see the module docs. `None` for 40-byte headers.
    pub fn tilt_range(&self) -> Option<i32> {
        self.slots.get(SLOT_TILT_RANGE).copied()
    }

    /// The pressure curve, if the blob has one.
    pub fn pressure_curve(&self) -> Option<&Curve> {
        self.pressure_curve.as_ref()
    }

    /// The tilt curve, if the blob has one.
    pub fn tilt_curve(&self) -> Option<&Curve> {
        self.tilt_curve.as_ref()
    }

    /// Replaces the pressure curve (`None` removes it); the header length slot follows on encode.
    pub fn set_pressure_curve(&mut self, curve: Option<Curve>) {
        self.pressure_curve = curve;
    }

    /// Replaces the tilt curve.
    pub fn set_tilt_curve(&mut self, curve: Option<Curve>) {
        self.tilt_curve = curve;
    }

    /// The raw header slots (`i32`), slot 0 being the header length.
    pub fn slots(&self) -> &[i32] {
        &self.slots
    }

    /// Bytes after the curves; empty in every observed blob.
    pub fn trailing(&self) -> &[u8] {
        &self.trailing
    }

    fn slot(&self, i: usize) -> i32 {
        self.slots.get(i).copied().unwrap_or(0)
    }

    fn set_slot(&mut self, i: usize, v: i32) {
        if let Some(s) = self.slots.get_mut(i) {
            *s = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut e = Effector::new(Sources(0x1f0));
        e.set_active(Sources(0x30));
        e.set_minimum(Source::Pressure, 30);
        e.set_pressure_curve(Some(Curve::linear()));
        e.set_tilt_curve(Some(Curve::new(vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)])));
        e.encode()
    }

    #[test]
    fn decode_encode_is_identity_and_reads_the_slots() {
        let bytes = sample();
        assert_eq!(bytes.len(), 44 + 44 + 60);
        let e = Effector::decode(&bytes).unwrap();
        assert_eq!(e.encode(), bytes);
        assert!(e.active().contains(Source::Pressure) && e.active().contains(Source::Tilt));
        assert!(!e.active().contains(Source::Velocity));
        assert_eq!(e.minimum(Source::Pressure), 30);
        assert_eq!(e.tilt_curve().unwrap().eval(0.5), Some(0.25));
        assert_eq!(e.tilt_curve().unwrap().eval(0.75), Some(0.625));
        assert_eq!(e.active().iter().collect::<Vec<_>>(), vec![Source::Pressure, Source::Tilt]);
    }

    #[test]
    fn every_truncation_is_an_error_or_a_shorter_valid_blob() {
        let bytes = sample();
        for cut in 0..bytes.len() {
            if let Ok(e) = Effector::decode(&bytes[..cut]) {
                assert_eq!(e.encode(), &bytes[..cut]);
            }
        }
    }

    #[test]
    fn odd_curve_layouts_and_trailing_bytes_are_kept() {
        let mut bytes = Effector::new(Sources(0x10)).encode();
        bytes[32..36].copy_from_slice(&8i32.to_be_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let e = Effector::decode(&bytes).unwrap();
        assert_eq!(e.pressure_curve().unwrap().opaque.as_deref(), Some(&[1, 2, 3, 4, 5, 6, 7, 8][..]));
        assert_eq!(e.trailing(), &[9]);
        assert_eq!(e.encode(), bytes);
    }

    #[test]
    fn hostile_headers_are_errors() {
        for header in [0i32, 3, 7, -4, 1 << 30, 46] {
            let mut bytes = Effector::new(Sources(0)).encode();
            bytes[0..4].copy_from_slice(&header.to_be_bytes());
            assert!(Effector::decode(&bytes).is_err(), "{header}");
        }
        let mut bytes = Effector::new(Sources(0)).encode();
        bytes[32..36].copy_from_slice(&(-1i32).to_be_bytes());
        assert!(Effector::decode(&bytes).is_err());
        bytes[32..36].copy_from_slice(&1000i32.to_be_bytes());
        assert!(Effector::decode(&bytes).is_err());
    }
}
