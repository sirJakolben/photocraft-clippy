//! The brush model: tip shape plus Photoshop-style dynamics sections, all plain serde data.
//!
//! Every section is optional (an `enabled` flag, or a neutral default), so a partial JSON object
//! such as `{"size": 40, "scattering": {"enabled": true, "scatter": {"jitter": 2.0}}}` deserialises
//! onto the defaults. Field names are camelCase in JSON, matching the command parameters.
//!
//! Units: sizes are pixels, angles are degrees, every percentage is a 0..1 fraction (scatter and
//! texture scale may exceed 1, like Photoshop's 1000 % scatter).

use photocraft_color::BlendMode;
use serde::{Deserialize, Serialize};

use crate::curve::ResponseCurve;
use crate::mixer::MixerSettings;
use crate::tile::{GrayTile, StoredTile};

/// Largest brush diameter accepted by the rasterizer and brush controls.
pub const MAX_BRUSH_SIZE: f32 = 5000.0;

/// Largest Scatter amount (primary and Dual Brush): 10 = 1000 %, the Brush Settings slider's top.
/// Beyond it a dab could land arbitrarily far away, and an unbounded amount overflowed the dab
/// centre to infinity (#977).
pub const MAX_SCATTER: f32 = 10.0;

/// What drives a dynamic parameter (Photoshop's "Control" pop-ups).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Control {
    #[default]
    Off,
    /// Ramp over `fadeSteps` spacing steps (from full to the minimum).
    Fade,
    PenPressure,
    PenTilt,
    StylusWheel,
    /// Barrel rotation of the stylus.
    Rotation,
    /// Angle only: the direction of the first stroke segment.
    InitialDirection,
    /// Angle only: the current stroke direction.
    Direction,
    /// The pen's speed (Clip Studio Paint's Velocity): full value while the pen moves slowly, down
    /// to nothing at [`crate::dynamics::VELOCITY_FULL_SPEED`] screen pixels per millisecond and
    /// faster. Input without timestamps reads full value, as a mouse reads no tilt.
    Velocity,
}

/// Most further controls one [`Dynamic`] combines with its own (Clip Studio Paint's pen settings
/// offer pressure, tilt and velocity at once, besides random).
pub const MAX_EXTRA_CONTROLS: usize = 3;

/// A further control of a [`Dynamic`]: its value, through `curve`, scaled into
/// `minimum..maximum` and multiplied with the dynamic's own control.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Influence {
    pub control: Control,
    /// 0..1 value at the control's weakest input.
    pub minimum: f32,
    /// 0..1 value at its strongest input (Clip Studio Paint's tilt has one; 1 for the others).
    pub maximum: f32,
    /// Response to the control (Fade uses the dynamic's `fade_steps`).
    pub curve: ResponseCurve,
}

impl Influence {
    const NONE: Self = Self { control: Control::Off, minimum: 0.0, maximum: 1.0, curve: ResponseCurve::LINEAR };

    pub const fn new(control: Control, minimum: f32) -> Self {
        Self { control, minimum, ..Self::NONE }
    }
}

impl Default for Influence {
    fn default() -> Self {
        Self::NONE
    }
}

/// A [`Dynamic`]'s further controls: up to [`MAX_EXTRA_CONTROLS`], never `Off` ones. Fixed-size, so
/// a [`Dynamic`] stays `Copy` and a dab evaluates it without allocating. In JSON it is a list of
/// [`Influence`] objects; extra entries and `off` controls are dropped when it is read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    n: u8,
    items: [Influence; MAX_EXTRA_CONTROLS],
}

impl Controls {
    pub const EMPTY: Self = Self { n: 0, items: [Influence::NONE; MAX_EXTRA_CONTROLS] };

    /// The first [`MAX_EXTRA_CONTROLS`] influences of `list` that have a control.
    pub fn new(list: &[Influence]) -> Self {
        let mut c = Self::EMPTY;
        for i in list.iter().filter(|i| i.control != Control::Off) {
            if !c.push(*i) {
                break;
            }
        }
        c
    }

    /// Add an influence; false (and nothing added) when full or `Off`.
    pub fn push(&mut self, i: Influence) -> bool {
        let Some(slot) = self.items.get_mut(usize::from(self.n)) else { return false };
        if i.control == Control::Off {
            return false;
        }
        *slot = i;
        self.n += 1;
        true
    }

    pub fn as_slice(&self) -> &[Influence] {
        self.items.get(..usize::from(self.n)).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Serialize for Controls {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Controls {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::new(&Vec::<Influence>::deserialize(d)?))
    }
}

/// One dynamic parameter: random jitter plus a controller with a floor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Dynamic {
    /// 0..1 random variation per dab (for scatter: the amount, may exceed 1).
    pub jitter: f32,
    pub control: Control,
    /// Steps for [`Control::Fade`].
    pub fade_steps: u32,
    /// 0..1 floor the control/jitter may reduce the value to (minimum diameter, roundness, depth…).
    pub minimum: f32,
    /// This dynamic's own response to its control (Fade, Pen Pressure, Pen Tilt, Stylus Wheel,
    /// Rotation, Velocity), applied after the global pen curve. Linear by default. Always written
    /// to JSON, so a `tools.setBrush` patch can set it back to linear. Angle Jitter's controls ignore
    /// it.
    pub curve: ResponseCurve,
    /// Further controls multiplied with `control` (Clip Studio Paint: several pen settings at once,
    /// such as pressure and velocity). Empty by default and then not written to JSON. The angle
    /// dynamic ignores them.
    #[serde(skip_serializing_if = "Controls::is_empty")]
    pub controls: Controls,
}

impl Default for Dynamic {
    fn default() -> Self {
        Self::jitter(0.0)
    }
}

impl Dynamic {
    pub const fn controlled(control: Control) -> Self {
        Self { jitter: 0.0, control, fade_steps: 25, minimum: 0.0, curve: ResponseCurve::LINEAR, controls: Controls::EMPTY }
    }
    pub const fn jitter(jitter: f32) -> Self {
        Self { jitter, control: Control::Off, fade_steps: 25, minimum: 0.0, curve: ResponseCurve::LINEAR, controls: Controls::EMPTY }
    }
    pub fn is_active(&self) -> bool {
        self.jitter > 0.0 || self.control != Control::Off || !self.controls.is_empty()
    }
}

/// The brush tip bitmap source.
///
/// A loaded [`TipShape::Stored`] tip serialises as `sampled` (its full bitmap), so tool presets,
/// recorded actions and `brush.get` see the same JSON as for an embedded tip.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TipShape {
    /// Computed round/elliptical tip (hardness, angle, roundness).
    #[default]
    Round,
    /// Sampled tip: grayscale bitmap where 1 = full paint. Scaled so its larger side equals `size`.
    Sampled(GrayTile),
    /// A sampled tip its owner (the preset store) keeps outside the settings and loads on demand.
    Stored(StoredTile),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum TipShapeOut<'a> {
    Round,
    Sampled(&'a GrayTile),
    Stored(&'a StoredTile),
}

impl Serialize for TipShape {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            TipShape::Round => TipShapeOut::Round,
            TipShape::Sampled(g) => TipShapeOut::Sampled(g),
            TipShape::Stored(r) => match r.full.as_deref() {
                Some(g) => TipShapeOut::Sampled(g),
                None => TipShapeOut::Stored(r),
            },
        }
        .serialize(s)
    }
}

impl TipShape {
    /// The bitmap a sampled tip paints with (a stored tip's full bitmap, or its preview while it
    /// isn't loaded); `None` for a computed tip.
    pub fn bitmap(&self) -> Option<&GrayTile> {
        match self {
            TipShape::Round => None,
            TipShape::Sampled(g) => Some(g),
            TipShape::Stored(r) => Some(r.bitmap()),
        }
    }
    /// The tip's real pixel size (a stored tip's, not its preview's); `None` for a computed tip.
    pub fn bitmap_size(&self) -> Option<(u32, u32)> {
        match self {
            TipShape::Round => None,
            TipShape::Sampled(g) => Some((g.width, g.height)),
            TipShape::Stored(r) => Some((r.width, r.height)),
        }
    }
}

/// Shape Dynamics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShapeDynamics {
    pub enabled: bool,
    /// Size jitter / control; `minimum` is Photoshop's Minimum Diameter.
    pub size: Dynamic,
    /// Tilt Scale, 0..2 (0–200 %): with the size control on Pen Tilt, how much a tilted pen
    /// squashes the tip's height (before the tip angle is applied). At full tilt the height is
    /// scaled by `1 − tiltScale / 2`, so 0 leaves the tip alone and 200 % flattens it.
    pub tilt_scale: f32,
    /// Angle jitter (fraction of ±180°) / control.
    pub angle: Dynamic,
    /// Roundness jitter / control; `minimum` is Minimum Roundness.
    pub roundness: Dynamic,
    pub flip_x_jitter: bool,
    pub flip_y_jitter: bool,
    /// Brush Projection: the pen's tilt and barrel rotation are applied to the tip as a
    /// projection (the tip is foreshortened along the tilt direction and turns with the pen).
    pub brush_projection: bool,
}

/// Scattering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scattering {
    pub enabled: bool,
    /// `jitter` = scatter amount (1.0 = 100 %: up to one radius of displacement); control scales it.
    pub scatter: Dynamic,
    /// Scatter radially instead of only perpendicular to the stroke.
    pub both_axes: bool,
    /// Dabs per spacing step (1..16).
    pub count: u32,
    /// Count jitter / control (reduces the count towards 1).
    pub count_jitter: Dynamic,
}

impl Default for Scattering {
    fn default() -> Self {
        Self { enabled: false, scatter: Dynamic::default(), both_axes: false, count: 1, count_jitter: Dynamic::default() }
    }
}

/// How a grayscale mask (texture or dual tip) combines with the brush coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaskMode {
    #[default]
    Multiply,
    Subtract,
    Darken,
    Overlay,
    ColorDodge,
    ColorBurn,
    LinearBurn,
    HardMix,
    LinearHeight,
    Height,
}

/// Procedural pattern styles (generated deterministically; no bundled pattern files).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatternStyle {
    /// Smooth value noise.
    #[default]
    Noise,
    /// Woven canvas.
    Canvas,
    /// Fibrous paper tooth.
    Paper,
    /// Halftone-like dots.
    Dots,
}

/// A texture pattern: a procedural generator or an explicit grayscale tile (tiled infinitely).
/// A loaded [`Pattern::Stored`] tile serialises as `tile`, like [`TipShape::Stored`].
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    Procedural {
        style: PatternStyle,
        size: u32,
        seed: u32,
    },
    Tile(GrayTile),
    /// A tile its owner (the preset store) keeps outside the settings and loads on demand.
    Stored(StoredTile),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum PatternOut<'a> {
    Procedural { style: PatternStyle, size: u32, seed: u32 },
    Tile(&'a GrayTile),
    Stored(&'a StoredTile),
}

impl Serialize for Pattern {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Pattern::Procedural { style, size, seed } => PatternOut::Procedural { style: *style, size: *size, seed: *seed },
            Pattern::Tile(g) => PatternOut::Tile(g),
            Pattern::Stored(r) => match r.full.as_deref() {
                Some(g) => PatternOut::Tile(g),
                None => PatternOut::Stored(r),
            },
        }
        .serialize(s)
    }
}

impl Pattern {
    /// The tile a bitmap pattern textures with (a stored tile's full bitmap, or its preview while
    /// it isn't loaded); `None` for a procedural pattern.
    pub fn bitmap(&self) -> Option<&GrayTile> {
        match self {
            Pattern::Procedural { .. } => None,
            Pattern::Tile(g) => Some(g),
            Pattern::Stored(r) => Some(r.bitmap()),
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern::Procedural { style: PatternStyle::Paper, size: 128, seed: 1 }
    }
}

/// Texture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Texture {
    pub enabled: bool,
    pub pattern: Pattern,
    pub invert: bool,
    /// Pattern scale (1 = 100 %).
    pub scale: f32,
    /// -1..1 (Photoshop's -150..150 mapped to ±1).
    pub brightness: f32,
    /// -1..1: Photoshop's -50..0 mapped to -1..0 and its 0..100 to 0..1.
    pub contrast: f32,
    /// Apply the texture to every dab (enables depth jitter) instead of once to the stroke.
    pub each_tip: bool,
    pub mode: MaskMode,
    /// 0..1.
    pub depth: f32,
    /// Depth jitter / control (per tip only); `minimum` is Minimum Depth.
    pub depth_jitter: Dynamic,
    /// Degrees the pattern is turned counter-clockwise (Clip Studio Paint's texture rotation).
    /// 0 by default and then not written to JSON.
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub rotation: f32,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

impl Texture {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The pattern's turn as `(sin, cos)` of the angle, `None` when it isn't turned (or the angle
    /// isn't finite).
    pub fn rotation_sin_cos(&self) -> Option<(f32, f32)> {
        let r = self.rotation.rem_euclid(360.0);
        (r.is_finite() && r != 0.0).then(|| r.to_radians().sin_cos())
    }

    /// A pattern value (0..1) after Brightness, Contrast and Invert: the value the mode combines
    /// with the brush. Measured against Photoshop (#3340): Brightness shifts the pattern by
    /// `brightness / 255` (±150 moves it 150 levels of 255, not all the way to white or black);
    /// positive Contrast steepens it around mid-grey by `1 / (1 − c/100)` after the shift (100 is
    /// a threshold), negative Contrast flattens it by `1 + c/100` before the shift; Invert is last.
    pub fn level(&self, v: f32) -> f32 {
        let b = if self.brightness.is_finite() { self.brightness.clamp(-1.0, 1.0) * (150.0 / 255.0) } else { 0.0 };
        let c = if self.contrast.is_finite() { self.contrast.clamp(-1.0, 1.0) } else { 0.0 };
        let x = if c >= 0.0 { (v + b - 0.5) / (1.0 - c).max(1e-4) + 0.5 } else { (v - 0.5) * (1.0 + c / 2.0) + 0.5 + b };
        let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.5 };
        if self.invert { 1.0 - x } else { x }
    }
}

impl Default for Texture {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: Pattern::default(),
            invert: false,
            scale: 1.0,
            brightness: 0.0,
            contrast: 0.0,
            each_tip: false,
            mode: MaskMode::Multiply,
            depth: 1.0,
            depth_jitter: Dynamic::default(),
            rotation: 0.0,
        }
    }
}

/// Dual Brush: a second tip stroked along the same path; the stroke is masked by it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DualBrush {
    pub enabled: bool,
    pub mode: MaskMode,
    pub tip: TipShape,
    pub size: f32,
    pub hardness: f32,
    pub roundness: f32,
    pub angle: f32,
    pub spacing: f32,
    pub scatter: f32,
    pub both_axes: bool,
    pub count: u32,
    pub flip: bool,
    /// A texture on the second tip's mask as a whole (Clip Studio Paint's dual brush texture);
    /// its `each_tip` and depth jitter don't apply. Off by default and then not written to JSON.
    #[serde(skip_serializing_if = "Texture::is_default")]
    pub texture: Texture,
}

impl Default for DualBrush {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: MaskMode::Multiply,
            tip: TipShape::Round,
            size: 20.0,
            hardness: 1.0,
            roundness: 1.0,
            angle: 0.0,
            spacing: 0.25,
            scatter: 0.0,
            both_axes: false,
            count: 1,
            flip: false,
            texture: Texture::default(),
        }
    }
}

/// Color Dynamics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorDynamics {
    pub enabled: bool,
    /// Vary every dab (true) or once per stroke (false).
    pub per_tip: bool,
    /// Foreground/background mix jitter / control.
    pub fg_bg: Dynamic,
    /// 0..1 fraction of ±half the hue wheel.
    pub hue_jitter: f32,
    pub saturation_jitter: f32,
    pub brightness_jitter: f32,
    /// -1 (fully desaturated) .. 1 (fully saturated).
    pub purity: f32,
}

impl Default for ColorDynamics {
    fn default() -> Self {
        Self { enabled: false, per_tip: true, fg_bg: Dynamic::default(), hue_jitter: 0.0, saturation_jitter: 0.0, brightness_jitter: 0.0, purity: 0.0 }
    }
}

/// Transfer: per-dab opacity ceiling and flow, plus the Mixer Brush's wetness and mix.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Transfer {
    pub enabled: bool,
    pub opacity: Dynamic,
    pub flow: Dynamic,
    /// Wetness Jitter / control (Mixer Brush only): scales the Wet setting per dab.
    pub wetness: Dynamic,
    /// Mix Jitter / control (Mixer Brush only): scales the Mix setting per dab.
    pub mix: Dynamic,
}

/// The sections a lock can hold (the lock icons in Photoshop's Brush Settings list): a locked
/// section keeps its settings when another brush preset is picked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SectionLocks {
    pub shape_dynamics: bool,
    pub scattering: bool,
    pub texture: bool,
    pub dual_brush: bool,
    pub color_dynamics: bool,
    pub transfer: bool,
    pub pose: bool,
    pub noise: bool,
    pub wet_edges: bool,
    pub build_up: bool,
    pub smoothing: bool,
    pub protect_texture: bool,
}

/// Brush Pose: stylus values used instead of (override) or for missing device data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pose {
    pub enabled: bool,
    /// Degrees, -90..90 (W3C Pointer Events convention).
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// Degrees 0..360.
    pub rotation: f32,
    /// 0..1.
    pub pressure: f32,
    pub override_tilt: bool,
    pub override_rotation: bool,
    pub override_pressure: bool,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            enabled: false,
            tilt_x: 0.0,
            tilt_y: 0.0,
            rotation: 0.0,
            pressure: 1.0,
            override_tilt: false,
            override_rotation: false,
            override_pressure: false,
        }
    }
}

/// Stroke smoothing (Photoshop's Smoothing % and its options).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Smoothing {
    /// Switch smoothing off without discarding its amount or mode. Old presets remain enabled.
    pub enabled: bool,
    /// 0..1.
    pub amount: f32,
    /// Lazy-mouse: paint only when the pointer pulls a string of `amount × 100 px` taut.
    pub pulled_string: bool,
    /// Keep catching up with the pointer while it pauses (uses point timestamps).
    pub catch_up: bool,
    /// Finish the stroke at the last pointer position.
    pub catch_up_on_end: bool,
    /// Divide the string length by the view zoom (the command's `zoom` param).
    pub adjust_for_zoom: bool,
    /// Clip Studio Paint's Stabilization › Adjust by speed: the amount follows the pointer's
    /// speed. `None` (the default, not written to JSON) keeps it fixed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_speed: Option<SpeedSmoothing>,
    /// Clip Studio Paint's Stickiness steps (0..=5; more read as 5): the brush keeps
    /// `stickiness × STICKINESS_PX` screen pixels ([`crate::dynamics::STICKINESS_PX`]) behind the
    /// pointer, so small wobbles don't move it, and catches up at the stroke's end. 0 (the
    /// default, not written to JSON) = none.
    #[serde(skip_serializing_if = "is_zero")]
    pub stickiness: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// How Smoothing follows the pointer's speed ([`Smoothing::by_speed`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeedSmoothing {
    /// Smooth more while the pointer moves slowly.
    StrengthenSlow,
    /// Smooth less while it moves fast.
    ReduceFast,
}

impl Default for Smoothing {
    fn default() -> Self {
        Self { enabled: true, amount: 0.0, pulled_string: false, catch_up: true, catch_up_on_end: true, adjust_for_zoom: true, by_speed: None, stickiness: 0 }
    }
}

fn is_true(v: &bool) -> bool {
    *v
}

/// How Wet Edges shapes the stroke: the paint left at a pixel goes from `interior` deep inside
/// the stroke up to all of it at the rim, over the outer `width` of a dab's radius.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WetEdge {
    /// 0..1 share of the paint the interior keeps (PhotoCraft: half).
    pub interior: f32,
    /// 0..1 share of the dab radius the darker rim spans (PhotoCraft: the outer half).
    pub width: f32,
}

impl Default for WetEdge {
    fn default() -> Self {
        Self { interior: 0.5, width: 0.5 }
    }
}

impl WetEdge {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `(interior, width)` clamped to 0..1, the defaults standing in for NaN; the width is at
    /// least a hundredth, so the rim never becomes a division by zero.
    pub fn sanitised(&self) -> (f32, f32) {
        let d = Self::default();
        let f = |v: f32, fallback: f32| if v.is_nan() { fallback } else { v.clamp(0.0, 1.0) };
        (f(self.interior, d.interior), f(self.width, d.width).max(0.01))
    }
}

/// Clip Studio Paint's Start and End: the stroke grows from nothing over its first `start` pixels
/// of path and shrinks to nothing over its last `end` pixels. Where the stroke ends is known only
/// when the pen lifts, so its last `end` pixels of dabs wait for that (a live preview shows them
/// tapered as if the pen lifted now). Off by default (both lengths 0).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Taper {
    /// Path length in pixels over which the start tapers; 0 = no start taper.
    pub start: f32,
    /// Path length in pixels over which the end tapers; 0 = no end taper.
    pub end: f32,
    /// Scale both lengths by the pen speed where they apply: full length from
    /// [`crate::dynamics::VELOCITY_FULL_SPEED`] on, shorter when slower. Input without timestamps
    /// tapers at full length.
    pub by_speed: bool,
    /// What tapers (Clip Studio Paint's target list): the size, the opacity ceiling, the flow.
    pub size: bool,
    pub opacity: bool,
    pub flow: bool,
}

impl Taper {
    /// Longest taper accepted, in pixels; longer ones are cut to it.
    pub const MAX_LENGTH: f32 = 10_000.0;

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Does the taper change the stroke at all?
    pub fn is_active(&self) -> bool {
        (self.start_len() > 0.0 || self.end_len() > 0.0) && (self.size || self.opacity || self.flow)
    }

    /// The start length, sanitised (finite, 0..[`Self::MAX_LENGTH`]).
    pub fn start_len(&self) -> f32 {
        Self::length(self.start)
    }

    /// The end length, sanitised (finite, 0..[`Self::MAX_LENGTH`]).
    pub fn end_len(&self) -> f32 {
        Self::length(self.end)
    }

    fn length(v: f32) -> f32 {
        if v.is_nan() { 0.0 } else { v.clamp(0.0, Self::MAX_LENGTH) }
    }
}

impl Default for Taper {
    fn default() -> Self {
        Self { start: 0.0, end: 0.0, by_speed: false, size: true, opacity: false, flow: false }
    }
}

/// A complete brush: tip shape, dynamics, and stroke-level options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrushSettings {
    /// Diameter in pixels.
    pub size: f32,
    /// 0 (soft) ..1 (hard). Computed tips only.
    pub hardness: f32,
    /// Spacing between dabs as a fraction of the diameter.
    pub spacing: f32,
    /// Photoshop's Spacing checkbox. Off: the pointer's speed sets the spacing (one dab every
    /// [`crate::dynamics::SPEED_SPACING_MS`] of stroke time; without timestamps, by distance).
    pub spacing_enabled: bool,
    /// Maximum coverage for the whole stroke.
    pub opacity: f32,
    /// Per-dab coverage.
    pub flow: f32,
    /// Shortcut for Shape Dynamics › Size › Pen Pressure (the options-bar button).
    #[serde(alias = "pressure_size")]
    pub pressure_size: bool,
    /// Shortcut for Transfer › Opacity › Pen Pressure.
    #[serde(alias = "pressure_opacity")]
    pub pressure_opacity: bool,
    /// Straight RGBA colour (display RGB). Commands fill it from the foreground colour.
    pub color: [f32; 4],
    /// The inks `color` was picked as (its sRGB colour, the CMYK values), for a CMYK target: the
    /// stroke paints exactly these, since an sRGB round trip does not give them back (#4344).
    /// Only valid while `color` is that sRGB colour; ignored with Color Dynamics.
    #[serde(skip)]
    pub color_cmyk: Option<([f32; 3], [f32; 4])>,
    /// Background colour for Color Dynamics' foreground/background jitter.
    pub background: [f32; 4],
    /// Erase instead of paint.
    pub erase: bool,
    /// Brush blend mode (Normal, Multiply, Dissolve…).
    pub mode: BlendMode,
    // --- Brush Tip Shape ---
    pub tip: TipShape,
    /// Degrees, counter-clockwise.
    pub angle: f32,
    /// 0..1 (1 = circle).
    pub roundness: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// Pencil: no anti-aliasing (coverage is 0 or 1).
    pub aliased: bool,
    /// With `aliased`, a square tip: every pixel of the dab's whole-pixel square
    /// ([`crate::grid_square`]), whatever its angle or roundness. The Eraser's Block mode.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub square: bool,
    /// Clip Studio Paint's Anti-aliasing steps: 0 none (an aliased tip, as `aliased`), 1 weak,
    /// 2 medium (PhotoCraft's own edge), 3 strong; larger values read as 3. `None` (the default,
    /// not written to JSON) leaves the edge to `aliased`, as before. A step sets how wide a
    /// computed tip's anti-aliased edge is at least ([`crate::aa_edge_px`]); a sampled tip keeps
    /// its bitmap's own edge unless the step is 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aa_level: Option<u8>,
    /// Clip Studio Paint's "At least 1 pixel". On (the default, as PhotoCraft always painted) a
    /// dab is never thinner than one pixel; off, a dab thinner than that fades with its diameter
    /// instead, so a stroke thinning out vanishes smoothly. Written to JSON only when off.
    #[serde(skip_serializing_if = "is_true")]
    pub min_one_px: bool,
    /// Clip Studio Paint's "Sync size with view": `size` is in screen pixels, so the dabs are
    /// `size / zoom` document pixels (the zoom the stroke is painted at). Off by default and then
    /// not written to JSON.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub size_follows_view: bool,
    /// Clip Studio Paint's "Blend brush strokes with darken": overlapping dabs keep the stronger
    /// coverage instead of building up. Off by default and then not written to JSON.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blend_darken: bool,
    // --- dynamics sections ---
    pub shape_dynamics: ShapeDynamics,
    pub scattering: Scattering,
    pub texture: Texture,
    pub dual_brush: DualBrush,
    pub color_dynamics: ColorDynamics,
    pub transfer: Transfer,
    pub pose: Pose,
    /// Add grain to the soft parts of the tip.
    pub noise: bool,
    /// Watercolour look: lighter interior, darker rims; overlapping dabs don't build up.
    pub wet_edges: bool,
    /// The Wet Edges shape (Clip Studio Paint's watercolour edge parameters); the default is
    /// PhotoCraft's own and then not written to JSON.
    #[serde(skip_serializing_if = "WetEdge::is_default")]
    pub wet_edge: WetEdge,
    /// Airbrush: keep depositing dabs over time while the pointer lingers.
    pub build_up: bool,
    /// Build-up rate in dabs per second.
    pub build_up_rate: f32,
    pub smoothing: Smoothing,
    /// Start and End taper (Clip Studio Paint); off by default and then not written to JSON.
    #[serde(skip_serializing_if = "Taper::is_default")]
    pub taper: Taper,
    /// Keep the current texture (pattern + scale) when switching to another textured preset.
    pub protect_texture: bool,
    /// Sections whose settings stay when another preset is picked (tool state, like Photoshop's locks).
    pub locks: SectionLocks,
    /// Mixer Brush options (Wet, Load, Mix, Flow, Sample All Layers) used by `paint.mixerBrush`.
    pub mixer: MixerSettings,
    /// Random seed for jitters (commands derive it from the stroke when not given), so strokes replay.
    pub seed: u64,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 20.0,
            hardness: 1.0,
            spacing: 0.1,
            spacing_enabled: true,
            opacity: 1.0,
            flow: 1.0,
            pressure_size: true,
            pressure_opacity: false,
            color: [0.0, 0.0, 0.0, 1.0],
            color_cmyk: None,
            background: [1.0, 1.0, 1.0, 1.0],
            erase: false,
            mode: BlendMode::Normal,
            tip: TipShape::Round,
            angle: 0.0,
            roundness: 1.0,
            flip_x: false,
            flip_y: false,
            aliased: false,
            square: false,
            aa_level: None,
            min_one_px: true,
            size_follows_view: false,
            blend_darken: false,
            shape_dynamics: ShapeDynamics::default(),
            scattering: Scattering::default(),
            texture: Texture::default(),
            dual_brush: DualBrush::default(),
            color_dynamics: ColorDynamics::default(),
            transfer: Transfer::default(),
            pose: Pose::default(),
            noise: false,
            wet_edges: false,
            wet_edge: WetEdge::default(),
            build_up: false,
            build_up_rate: 20.0,
            smoothing: Smoothing::default(),
            taper: Taper::default(),
            protect_texture: false,
            locks: SectionLocks::default(),
            mixer: MixerSettings::default(),
            seed: 0,
        }
    }
}

impl BrushSettings {
    /// Does the tip paint without anti-aliasing (coverage 0 or 1)? [`Self::aa_level`] decides
    /// when set, else [`Self::aliased`].
    pub fn is_aliased(&self) -> bool {
        match self.aa_level {
            Some(level) => level == 0,
            None => self.aliased,
        }
    }

    /// Resize the primary tip and keep an enabled Dual Brush tip at the same relative size.
    /// An explicit Dual Brush size edit should bypass this and retain the requested diameter.
    ///
    /// As in Photoshop, the dual diameter is multiplied by the size ratio unrounded and only capped
    /// at the top ([`MAX_BRUSH_SIZE`]): a 100 px brush with a 20 px dual tip shrunk to 1 px has a
    /// 0.2 px dual tip and grows back to exactly 20 px. A sub-pixel floor here would lose the
    /// proportion on the way back up (#4809); rendering bounds the diameter itself
    /// ([`Self::bounded_for_render`]).
    pub fn resize_with_dual_brush(&mut self, new_size: f32) {
        if self.dual_brush.enabled && self.size.is_finite() && self.size > 0.0 && new_size.is_finite() && new_size > 0.0 {
            let scaled = self.dual_brush.size * (new_size / self.size);
            // NaN fails the comparison; infinity is capped.
            if scaled > 0.0 {
                self.dual_brush.size = scaled.min(MAX_BRUSH_SIZE);
            }
        }
        self.size = new_size;
    }

    /// The settings a stroke painted at view `zoom` uses: with [`Self::size_follows_view`] the
    /// sizes are screen pixels and become `size / zoom` document pixels (bounded by
    /// [`Self::bounded_for_render`] afterwards); otherwise unchanged.
    pub fn for_view(&self, zoom: f32) -> std::borrow::Cow<'_, Self> {
        if !self.size_follows_view || !(zoom.is_finite() && zoom > 0.0) || zoom == 1.0 {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut b = self.clone();
        b.size /= zoom;
        b.dual_brush.size /= zoom;
        std::borrow::Cow::Owned(b)
    }

    /// Copy the settings with the primary and dual diameters and Scatter amounts constrained for
    /// rasterization.
    ///
    /// Engine commands reject out-of-range values; this is a final guard for direct users of the
    /// infallible paint API, which must not turn malformed brush dimensions into giant allocations,
    /// nor a huge Scatter amount into non-finite dab centres (#977).
    pub fn bounded_for_render(&self) -> Self {
        let safe_size = |size: f32| {
            if size.is_finite() { size.clamp(0.5, MAX_BRUSH_SIZE) } else { 0.5 }
        };
        // NaN scatters nothing (as before: it never passed the `> 0` check); infinity is the maximum.
        let safe_scatter = |amount: f32| if amount.is_nan() { 0.0 } else { amount.clamp(0.0, MAX_SCATTER) };
        let mut brush = self.clone();
        brush.size = safe_size(brush.size);
        brush.dual_brush.size = safe_size(brush.dual_brush.size);
        brush.scattering.scatter.jitter = safe_scatter(brush.scattering.scatter.jitter);
        brush.dual_brush.scatter = safe_scatter(brush.dual_brush.scatter);
        brush
    }

    /// This brush (a preset) picked while `current` is the tool's brush: Smoothing is a tool
    /// option, so it stays the tool's (Photoshop), a protected texture stays too, and so does every
    /// section `current` has locked (the locks themselves are tool state and carry over).
    pub fn picked_over(self, current: &BrushSettings) -> Self {
        let mut b = self.with_protected_texture(current);
        b.smoothing = current.smoothing.clone();
        let l = &current.locks;
        if l.shape_dynamics {
            b.shape_dynamics = current.shape_dynamics.clone();
        }
        if l.scattering {
            b.scattering = current.scattering.clone();
        }
        if l.texture {
            b.texture = current.texture.clone();
        }
        if l.dual_brush {
            b.dual_brush = current.dual_brush.clone();
        }
        if l.color_dynamics {
            b.color_dynamics = current.color_dynamics.clone();
        }
        if l.transfer {
            b.transfer = current.transfer.clone();
        }
        if l.pose {
            b.pose = current.pose.clone();
        }
        if l.noise {
            b.noise = current.noise;
        }
        if l.wet_edges {
            b.wet_edges = current.wet_edges;
        }
        if l.build_up {
            b.build_up = current.build_up;
            b.build_up_rate = current.build_up_rate;
        }
        if l.protect_texture {
            b.protect_texture = current.protect_texture;
        }
        b.locks = current.locks.clone();
        b
    }

    /// Apply Protect Texture: if `current` protects its texture, a newly chosen brush with a texture
    /// keeps `current`'s pattern and scale.
    pub fn with_protected_texture(mut self, current: &BrushSettings) -> Self {
        if current.protect_texture && current.texture.enabled && self.texture.enabled {
            self.texture.pattern = current.texture.pattern.clone();
            self.texture.scale = current.texture.scale;
            self.protect_texture = true;
        }
        self
    }
}

/// A named brush preset (session tool state; serde round-trips).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrushPreset {
    pub name: String,
    pub brush: BrushSettings,
    /// Shipped with PhotoCraft (built-ins can be deleted from a session but are regenerated on reset).
    #[serde(default)]
    pub builtin: bool,
    /// Preset group (folder) in the Brushes panel, e.g. "General" or an imported file's name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// Folders inside `group`, outermost first (empty = directly in the group), like the nested
    /// folders of Photoshop's Brushes panel; e.g. an imported `.abr` file's own folders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folder: Vec<String>,
    /// What picking the preset brings besides the tip and its sections: the New Brush Preset
    /// dialog's boxes.
    #[serde(default, skip_serializing_if = "PresetCaptures::is_default")]
    pub captures: PresetCaptures,
}

/// The New Brush Preset dialog's boxes, kept with the preset. The default is a preset made
/// without the dialog (a built-in, an import, an older save): picking it brings its size and tool
/// settings, as it always did, and leaves the foreground colour alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PresetCaptures {
    /// Capture Brush Size in Preset. Off: picking the preset keeps the tool's size.
    pub size: bool,
    /// Include Tool Settings: the options bar's mode, opacity, flow and pressure buttons. Off:
    /// picking the preset keeps the tool's.
    pub tool_settings: bool,
    /// Include Color: picking the preset sets the foreground colour to the brush's `color`.
    pub color: bool,
}

impl Default for PresetCaptures {
    fn default() -> Self {
        Self { size: true, tool_settings: true, color: false }
    }
}

impl PresetCaptures {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl BrushPreset {
    /// The tool's brush once this preset is picked while `current` is the tool's brush: the
    /// preset's brush ([`BrushSettings::picked_over`]), with the size and tool settings it didn't
    /// capture left as they were.
    pub fn picked_over(&self, current: &BrushSettings) -> BrushSettings {
        let mut b = self.brush.clone().picked_over(current);
        if !self.captures.size {
            b.resize_with_dual_brush(current.size);
        }
        if !self.captures.tool_settings {
            b.mode = current.mode;
            b.opacity = current.opacity;
            b.flow = current.flow;
            b.pressure_size = current.pressure_size;
            b.pressure_opacity = current.pressure_opacity;
        }
        b
    }
}
