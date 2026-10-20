//! `Variant` columns PhotoCraft maps, with their layout. Names as Clip Studio Paint 1.5 spells them
//! (including its typos, e.g. `BrushVerticalThicknes`, `Stickness`). Columns not listed here are
//! still read and written; they just have no typed meaning yet.

/// How a column's value is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `INTEGER` (also flags and enums).
    Int,
    /// `REAL`.
    Real,
    /// [`crate::Effector`] blob.
    Effector,
    /// [`crate::MaterialRefs`] blob.
    MaterialRefs,
    /// [`crate::ShowParams`] blob.
    ShowParams,
    /// Any other blob (kept verbatim).
    Blob,
}

/// The layout of a column, from its name and declared type.
pub fn kind(column: &str, decl_type: &str) -> Kind {
    if column == SHOW_PARAM {
        Kind::ShowParams
    } else if column.ends_with("Effector") && decl_type.eq_ignore_ascii_case("BLOB") {
        Kind::Effector
    } else if MATERIAL_COLUMNS.contains(&column) {
        Kind::MaterialRefs
    } else if decl_type.eq_ignore_ascii_case("INTEGER") {
        Kind::Int
    } else if decl_type.eq_ignore_ascii_case("REAL") {
        Kind::Real
    } else {
        Kind::Blob
    }
}

/// Eye toggles of the Tool Property palette.
pub const SHOW_PARAM: &str = "VariantShowParam";
/// Material list columns.
pub const MATERIAL_COLUMNS: [&str; 4] = [BRUSH_PATTERN_IMAGES, TEXTURE_IMAGE, DUAL_PATTERN_IMAGES, DUAL_TEXTURE_IMAGE];

/// Table of embedded materials in a `.sut`.
pub const MATERIAL_TABLE: &str = "MaterialFile";
/// Definition of [`MATERIAL_TABLE`] as Clip Studio Paint 1.5 writes it.
pub const MATERIAL_FILE_SQL: &str = "CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, InstallFolder INTEGER DEFAULT NULL, OriginalPath TEXT DEFAULT NULL, OldMaterial INTEGER DEFAULT NULL, FileData BLOB DEFAULT NULL, CatalogPath TEXT DEFAULT NULL, MaterialUuid TEXT DEFAULT NULL)";
/// Columns of [`MATERIAL_FILE_SQL`].
pub const MATERIAL_FILE_COLUMNS: [(&str, &str); 7] = [
    ("_PW_ID", "INTEGER"),
    ("InstallFolder", "INTEGER"),
    ("OriginalPath", "TEXT"),
    ("OldMaterial", "INTEGER"),
    ("FileData", "BLOB"),
    ("CatalogPath", "TEXT"),
    ("MaterialUuid", "TEXT"),
];

// Size, ink, colour (docs/einstellungen-abgleich.md, "Pinsel-Kategorien").
pub const BRUSH_SIZE: &str = "BrushSize";
pub const BRUSH_SIZE_UNIT: &str = "BrushSizeUnit";
pub const BRUSH_SIZE_EFFECTOR: &str = "BrushSizeEffector";
pub const BRUSH_AT_LEAST_1_PIXEL: &str = "BrushAtLeast1Pixel";
pub const BRUSH_SIZE_SYNC_VIEW_SCALE: &str = "BrushSizeSyncViewScale";
pub const OPACITY: &str = "Opacity";
pub const BRUSH_OPACITY_EFFECTOR: &str = "BrushOpacityEffector";
pub const COMPOSITE_MODE: &str = "CompositeMode";
pub const BRUSH_MIX_COLOR: &str = "BrushMixColor";
pub const BRUSH_MIX_COLOR_EFFECTOR: &str = "BrushMixColorEffector";
pub const BRUSH_MIX_ALPHA: &str = "BrushMixAlpha";
pub const BRUSH_MIX_ALPHA_EFFECTOR: &str = "BrushMixAlphaEffector";
pub const BRUSH_MIX_COLOR_EXTENSION: &str = "BrushMixColorExtension";
pub const BRUSH_BLUR: &str = "BrushBlur";
pub const BRUSH_BLUR_EFFECTOR: &str = "BrushBlurEffector";
pub const BRUSH_HUE_CHANGE: &str = "BrushHueChange";
pub const BRUSH_HUE_CHANGE_EFFECTOR: &str = "BrushHueChangeEffector";
pub const BRUSH_SATURATION_CHANGE: &str = "BrushSaturationChange";
pub const BRUSH_SATURATION_CHANGE_EFFECTOR: &str = "BrushSaturationChangeEffector";
pub const BRUSH_VALUE_CHANGE: &str = "BrushValueChange";
pub const BRUSH_VALUE_CHANGE_EFFECTOR: &str = "BrushValueChangeEffector";
pub const BRUSH_SUB_COLOR: &str = "BrushSubColor";
pub const BRUSH_SUB_COLOR_EFFECTOR: &str = "BrushSubColorEffector";
pub const BRUSH_CHANGE_STROKE_COLOR: &str = "BrushChangeStrokeColor";
pub const ANTI_ALIAS: &str = "AntiAlias";

// Tip shape.
pub const BRUSH_USE_PATTERN_IMAGE: &str = "BrushUsePatternImage";
pub const BRUSH_PATTERN_IMAGES: &str = "BrushPatternImageArray";
pub const BRUSH_PATTERN_ORDER_TYPE: &str = "BrushPatternOrderType";
pub const BRUSH_HARDNESS: &str = "BrushHardness";
pub const BRUSH_THICKNESS: &str = "BrushThickness";
pub const BRUSH_THICKNESS_EFFECTOR: &str = "BrushThicknessEffector";
pub const BRUSH_VERTICAL_THICKNESS: &str = "BrushVerticalThicknes";
pub const BRUSH_ROTATION: &str = "BrushRotation";
pub const BRUSH_ROTATION_EFFECTOR: &str = "BrushRotationEffector";
pub const BRUSH_ROTATION_RANDOM_SCALE: &str = "BrushRotationRandomScale";
pub const BRUSH_PATTERN_REVERSE_HORIZONTAL: &str = "BrushPatternReverseHorizontal";
pub const BRUSH_PATTERN_REVERSE_VERTICAL: &str = "BrushPatternReverseVertical";
pub const BRUSH_FLOW: &str = "BrushFlow";
pub const BRUSH_FLOW_EFFECTOR: &str = "BrushFlowEffector";
pub const BRUSH_ADJUST_FLOW_BY_INTERVAL: &str = "BrushAdjustFlowByInterval";

// Spray and stroke.
pub const BRUSH_USE_SPRAY: &str = "BrushUseSpray";
pub const BRUSH_SPRAY_SIZE: &str = "BrushSpraySize";
pub const BRUSH_SPRAY_SIZE_EFFECTOR: &str = "BrushSpraySizeEffector";
pub const BRUSH_SPRAY_DENSITY: &str = "BrushSprayDensity";
pub const BRUSH_SPRAY_DENSITY_EFFECTOR: &str = "BrushSprayDensityEffector";
pub const BRUSH_SPRAY_BIAS: &str = "BrushSprayBias";
pub const BRUSH_INTERVAL: &str = "BrushInterval";
pub const BRUSH_INTERVAL_EFFECTOR: &str = "BrushIntervalEffector";
pub const BRUSH_AUTO_INTERVAL_TYPE: &str = "BrushAutoIntervalType";
pub const BRUSH_CONTINUOUS_PLOT: &str = "BrushContinuousPlot";
pub const BRUSH_ADJUST_VELOCITY: &str = "BrushAdjustVelocity";
pub const BRUSH_RIBBON: &str = "BrushRibbon";
pub const BRUSH_BLEND_PATTERN_BY_DARKEN: &str = "BrushBlendPatternByDarken";

// Texture.
pub const TEXTURE_IMAGE: &str = "TextureImage";
pub const TEXTURE_DENSITY: &str = "TextureDensity";
pub const TEXTURE_DENSITY_EFFECTOR: &str = "TextureDensityEffector";
pub const TEXTURE_REVERSE_DENSITY: &str = "TextureReverseDensity";
pub const TEXTURE_STRESS_DENSITY: &str = "TextureStressDensity";
pub const TEXTURE_SCALE: &str = "TextureScale";
pub const TEXTURE_SCALE_2: &str = "TextureScale2";
pub const TEXTURE_ROTATE: &str = "TextureRotate";
pub const TEXTURE_BRIGHTNESS: &str = "TextureBrightness";
pub const TEXTURE_CONTRAST: &str = "TextureContrast";
pub const TEXTURE_COMPOSITE_MODE: &str = "TextureCompositeMode";
pub const TEXTURE_FOR_PLOT: &str = "TextureForPlot";

// Dual brush.
pub const USE_DUAL_BRUSH: &str = "UseDualBrush";
pub const DUAL_PATTERN_IMAGES: &str = "DualPatternImageArray";
pub const DUAL_TEXTURE_IMAGE: &str = "DualTextureImage";
pub const DUAL_SIZE: &str = "DualSize";
pub const DUAL_SIZE_EFFECTOR: &str = "DualSizeEffector";
pub const DUAL_BRUSH_COMPOSITE_MODE: &str = "DualBrushCompositeMode";

// Watercolor edge, eraser, correction, start and end.
pub const BRUSH_USE_WATER_EDGE: &str = "BrushUseWaterEdge";
pub const BRUSH_WATER_EDGE_RADIUS: &str = "BrushWaterEdgeRadius";
pub const BRUSH_WATER_EDGE_ALPHA_POWER: &str = "BrushWaterEdgeAlphaPower";
pub const BRUSH_WATER_EDGE_VALUE_POWER: &str = "BrushWaterEdgeValuePower";
pub const BRUSH_WATER_EDGE_BLUR: &str = "BrushWaterEdgeBlur";
pub const BRUSH_USE_VECTOR_ERASER: &str = "BrushUseVectorEraser";
pub const BRUSH_ERASE_ALL_LAYER: &str = "BrushEraseAllLayer";
pub const FLICKER_REDUCTION: &str = "FlickerReduction";
pub const FLICKER_REDUCTION_BY_SPEED: &str = "FlickerReductionBySpeed";
pub const FLICKER_REDUCTION_BY_SPEED_TYPE: &str = "FlickerReductionBySpeedType";
pub const BRUSH_USE_REVISION: &str = "BrushUseRevision";
pub const BRUSH_SHARPEN_CORNER: &str = "BrushSharpenCorner";
pub const BRUSH_ENABLE_SNAP: &str = "BrushEnableSnap";
pub const BRUSH_USE_VECTOR_MAGNET: &str = "BrushUseVectorMagnet";
pub const STICKNESS: &str = "Stickness";
pub const BRUSH_USE_IN: &str = "BrushUseIn";
pub const BRUSH_USE_OUT: &str = "BrushUseOut";
pub const BRUSH_IN_LENGTH: &str = "BrushInLength";
pub const BRUSH_OUT_LENGTH: &str = "BrushOutLength";
pub const BRUSH_IN_RATIO: &str = "BrushInRatio";
pub const BRUSH_OUT_RATIO: &str = "BrushOutRatio";
pub const BRUSH_IN_OUT_TARGET: &str = "BrushInOutTarget";
pub const BRUSH_IN_OUT_BY_SPEED: &str = "BrushInOutBySpeed";
pub const BRUSH_USE_REFER_LAYER: &str = "BrushUseReferLayer";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_name_and_declared_type() {
        assert_eq!(kind(BRUSH_SIZE_EFFECTOR, "BLOB"), Kind::Effector);
        assert_eq!(kind(BRUSH_ROTATION_EFFECTOR, "INTEGER"), Kind::Int);
        assert_eq!(kind(TEXTURE_IMAGE, ""), Kind::MaterialRefs);
        assert_eq!(kind(SHOW_PARAM, "BLOB"), Kind::ShowParams);
        assert_eq!(kind(BRUSH_SIZE, "REAL"), Kind::Real);
        assert_eq!(kind("GradationData", "BLOB"), Kind::Blob);
    }
}
