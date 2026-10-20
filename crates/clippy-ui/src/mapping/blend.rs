//! Clip Studio Paint blending mode numbers ↔ PhotoCraft's.
//!
//! `CompositeMode` uses the numbering of Clip Studio Paint's layer blending modes (as its project
//! files store them). Glow Dodge and Add (Glow) have no exact PhotoCraft mode and read as the
//! nearest one; writing such a nearest mode back keeps the stored number, so a round trip leaves
//! the column alone. Numbers without an entry read as Normal and are never overwritten unless
//! the user picks another mode. The texture and dual brush modes are a separate, shorter list
//! (an assumption until checked on real files).

use photocraft_color::BlendMode;
use photocraft_engine::paint::brush::MaskMode;

/// (Clip Studio Paint number, PhotoCraft mode). The first entry of a mode is the one written.
pub const COMPOSITE: [(i64, BlendMode); 28] = [
    (0, BlendMode::Normal),
    (1, BlendMode::Darken),
    (2, BlendMode::Multiply),
    (3, BlendMode::ColorBurn),
    (4, BlendMode::LinearBurn),
    (5, BlendMode::Subtract),
    (6, BlendMode::DarkerColor),
    (7, BlendMode::Lighten),
    (8, BlendMode::Screen),
    (9, BlendMode::ColorDodge),
    // Glow Dodge: Color Dodge that doesn't burn out semi-transparent paint.
    (10, BlendMode::ColorDodge),
    (11, BlendMode::LinearDodge),
    // Add (Glow).
    (12, BlendMode::LinearDodge),
    (13, BlendMode::LighterColor),
    (14, BlendMode::Overlay),
    (15, BlendMode::SoftLight),
    (16, BlendMode::HardLight),
    (17, BlendMode::VividLight),
    (18, BlendMode::LinearLight),
    (19, BlendMode::PinLight),
    (20, BlendMode::HardMix),
    (21, BlendMode::Difference),
    (22, BlendMode::Exclusion),
    (23, BlendMode::Hue),
    (24, BlendMode::Saturation),
    (25, BlendMode::Color),
    (26, BlendMode::Luminosity),
    (30, BlendMode::Divide),
];

/// The PhotoCraft mode of a Clip Studio Paint number (`None`: no counterpart).
pub fn mode_of(number: i64) -> Option<BlendMode> {
    COMPOSITE.iter().find(|(n, _)| *n == number).map(|(_, m)| *m)
}

/// The number written for a PhotoCraft mode (`None`: Clip Studio Paint has no such mode).
pub fn number_of(mode: BlendMode) -> Option<i64> {
    COMPOSITE.iter().find(|(_, m)| *m == mode).map(|(n, _)| *n)
}

/// Texture and dual brush modes (`TextureCompositeMode`, `DualBrushCompositeMode`).
pub const MASK: [(i64, MaskMode); 9] = [
    (0, MaskMode::Multiply),
    (1, MaskMode::Subtract),
    (2, MaskMode::Darken),
    (3, MaskMode::Overlay),
    (4, MaskMode::ColorDodge),
    (5, MaskMode::ColorBurn),
    (6, MaskMode::LinearBurn),
    (7, MaskMode::HardMix),
    (8, MaskMode::Height),
];

pub fn mask_of(number: i64) -> Option<MaskMode> {
    MASK.iter().find(|(n, _)| *n == number).map(|(_, m)| *m)
}

pub fn mask_number(mode: MaskMode) -> Option<i64> {
    MASK.iter().find(|(_, m)| *m == mode).map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_written_number_reads_back_as_its_mode() {
        for (_, mode) in COMPOSITE {
            assert_eq!(mode_of(number_of(mode).unwrap()), Some(mode));
        }
        for (_, mode) in MASK {
            assert_eq!(mask_of(mask_number(mode).unwrap()), Some(mode));
        }
        assert_eq!(mode_of(10), Some(BlendMode::ColorDodge));
        assert_eq!(number_of(BlendMode::ColorDodge), Some(9));
        assert_eq!(mode_of(-1), None);
        assert_eq!(number_of(BlendMode::Dissolve), None);
    }
}
