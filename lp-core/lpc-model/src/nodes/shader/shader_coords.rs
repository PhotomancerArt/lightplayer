//! Which coordinate frame a shader's `pos` argument arrives in.

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotEnumOption, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError,
};
use alloc::string::ToString;

/// The frame a shader's `render_2d` / `render_1d` argument is expressed in.
///
/// Authored as the optional `coords` key on [`ShaderDef`](crate::ShaderDef).
/// **Absence is [`ShaderCoords::Pixels`]**: every shader written before the
/// key existed keeps exactly the `pos` and `outputSize` it always saw, so old
/// files round-trip byte-identically and render byte-identically.
///
/// [`ShaderCoords::Pattern`] opts the shader into *pattern space* (ADR
/// `docs/adr/2026-09-24-pattern-space.md`): the lamps' bounding box, centred
/// at the origin, scaled uniformly so its long side spans −1…1, y up — and in
/// 1D, 0 → 1 from the strand's first lamp to its last. Alongside `pos` the
/// engine supplies three render-request intrinsics a shader may declare:
/// `uniform vec2 patternExtent;` (half-size of the lamp box in pattern
/// units), `uniform float patternPitch;` (mean consecutive-lamp distance in
/// pattern units) and `uniform float lampCount;`.
///
/// The transform is applied host-side to the sample coordinates, never
/// compiled into the program, so flipping this key costs no recompile and
/// every backend sees the same coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderCoords {
    /// Today's frame: `pos` in render-request pixels (pixel centres at
    /// `+0.5`), `outputSize` the request's size.
    #[default]
    Pixels,
    /// Pattern space: `pos` normalized to the lamps, plus scope geometry.
    Pattern,
}

const SHADER_COORDS_LABEL: &str = "Coordinates";

const SHADER_COORDS_DESCRIPTION: &str = concat!(
    "Unset = Pixels: pos arrives in render pixels. Pattern: pos arrives ",
    "normalized to the lamps (long side -1..1, y up; 0..1 along a strip), ",
    "with patternExtent, patternPitch and lampCount available as uniforms."
);

const PIXELS_LABEL: &str = "Pixels";
const PATTERN_LABEL: &str = "Pattern space";

impl ShaderCoords {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pixels => "pixels",
            Self::Pattern => "pattern",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "pixels" => Ok(Self::Pixels),
            "pattern" => Ok(Self::Pattern),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown shader coords {other:?} (expected \"pixels\" or \"pattern\")"
            ))),
        }
    }

    /// Whether `pos` arrives in pattern space.
    #[must_use]
    pub fn is_pattern(self) -> bool {
        self == Self::Pattern
    }
}

impl ToLpValue for ShaderCoords {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for ShaderCoords {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(value.as_str()),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ShaderCoords {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ShaderCoords");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta {
                label: Some(SHADER_COORDS_LABEL),
                description: Some(SHADER_COORDS_DESCRIPTION),
                unit: None,
            },
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "pixels",
                        label: PIXELS_LABEL,
                    },
                    StaticSlotEnumOption {
                        value: "pattern",
                        label: PATTERN_LABEL,
                    },
                ],
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta {
                label: Some(SHADER_COORDS_LABEL.to_string()),
                description: Some(SHADER_COORDS_DESCRIPTION.to_string()),
                unit: None,
            },
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("pixels", PIXELS_LABEL),
                    SlotEnumOption::new("pattern", PATTERN_LABEL),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_pixels() {
        assert_eq!(ShaderCoords::default(), ShaderCoords::Pixels);
    }

    #[test]
    fn round_trips_through_lp_value() {
        for coords in [ShaderCoords::Pixels, ShaderCoords::Pattern] {
            let value = coords.to_lp_value();
            assert_eq!(ShaderCoords::from_lp_value(&value).unwrap(), coords);
        }
    }

    #[test]
    fn parse_rejects_unknown() {
        assert!(ShaderCoords::parse("normalized").is_err());
    }
}
