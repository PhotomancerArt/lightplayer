use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::nodes::fixture::{FixtureSamplingConfig, MappingConfig};
use crate::{
    Affine2dSlot, BindingDefs, Dim2u, Dim2uSlot, EnumSlot, FromLpValue, LpType, LpValue,
    OptionSlot, SlotEnumOption, SlotMeta, SlotRecord, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Authored fixture node definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, SlotRecord)]
pub struct FixtureDef {
    /// Full-frame render size used when materializing the fixture input.
    #[serde(default = "default_render_size")]
    pub render_size: Dim2uSlot,
    /// Authored slot bindings for fixture inputs.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    /// Visual sampling strategy.
    #[serde(default)]
    pub sampling: ValueSlot<FixtureSamplingConfig>,
    /// Fixture mapping definition.
    pub mapping: EnumSlot<MappingConfig>,
    /// Color order for RGB channels.
    pub color_order: ValueSlot<ColorOrder>,
    /// Texture-space 2D affine transform.
    pub transform: Affine2dSlot,
    /// Brightness level (0-255).
    #[serde(default = "default_brightness")]
    pub brightness: OptionSlot<ValueSlot<u32>>,
    /// Enable gamma correction.
    #[serde(default = "default_gamma_correction")]
    pub gamma_correction: OptionSlot<ValueSlot<bool>>,
}

impl FixtureDef {
    pub const KIND: &'static str = "fixture";

    pub fn render_width(&self) -> u32 {
        self.render_size.value().width
    }

    pub fn render_height(&self) -> u32 {
        self.render_size.value().height
    }

    pub fn color_order(&self) -> ColorOrder {
        *self.color_order.value()
    }

    pub fn brightness_u8(&self) -> u8 {
        self.brightness
            .data
            .as_ref()
            .and_then(|value| u8::try_from(*value.value()).ok())
            .unwrap_or(64)
    }

    pub fn gamma_correction(&self) -> bool {
        self.gamma_correction
            .data
            .as_ref()
            .is_none_or(|value| *value.value())
    }

    pub fn transform_matrix(&self) -> [[f32; 4]; 4] {
        let transform = self.transform.value();
        [
            [transform.m00, transform.m01, 0.0, transform.tx],
            [transform.m10, transform.m11, 0.0, transform.ty],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Fixture
    }
}

fn default_brightness() -> OptionSlot<ValueSlot<u32>> {
    OptionSlot::some(ValueSlot::new(64_u32))
}

fn default_render_size() -> Dim2uSlot {
    Dim2uSlot::new(Dim2u {
        width: 16,
        height: 16,
    })
}

fn default_gamma_correction() -> OptionSlot<ValueSlot<bool>> {
    OptionSlot::some(ValueSlot::new(true))
}

/// Color order for RGB channels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorOrder {
    /// Red, Green, Blue.
    Rgb,
    /// Green, Red, Blue.
    #[default]
    Grb,
    /// Red, Blue, Green.
    Rbg,
    /// Green, Blue, Red.
    Gbr,
    /// Blue, Red, Green.
    Brg,
    /// Blue, Green, Red.
    Bgr,
}

impl ColorOrder {
    /// Get color order as string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorOrder::Rgb => "rgb",
            ColorOrder::Grb => "grb",
            ColorOrder::Rbg => "rbg",
            ColorOrder::Gbr => "gbr",
            ColorOrder::Brg => "brg",
            ColorOrder::Bgr => "bgr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rgb" => Some(Self::Rgb),
            "grb" => Some(Self::Grb),
            "rbg" => Some(Self::Rbg),
            "gbr" => Some(Self::Gbr),
            "brg" => Some(Self::Brg),
            "bgr" => Some(Self::Bgr),
            _ => None,
        }
    }

    /// Get bytes per pixel.
    pub fn bytes_per_pixel(&self) -> usize {
        3
    }

    /// Write RGB values to buffer in the correct order.
    pub fn write_rgb(&self, buffer: &mut [u8], offset: usize, r: u8, g: u8, b: u8) {
        if offset + 3 > buffer.len() {
            return;
        }
        match self {
            ColorOrder::Rgb => {
                buffer[offset] = r;
                buffer[offset + 1] = g;
                buffer[offset + 2] = b;
            }
            ColorOrder::Grb => {
                buffer[offset] = g;
                buffer[offset + 1] = r;
                buffer[offset + 2] = b;
            }
            ColorOrder::Rbg => {
                buffer[offset] = r;
                buffer[offset + 1] = b;
                buffer[offset + 2] = g;
            }
            ColorOrder::Gbr => {
                buffer[offset] = g;
                buffer[offset + 1] = b;
                buffer[offset + 2] = r;
            }
            ColorOrder::Brg => {
                buffer[offset] = b;
                buffer[offset + 1] = r;
                buffer[offset + 2] = g;
            }
            ColorOrder::Bgr => {
                buffer[offset] = b;
                buffer[offset + 1] = g;
                buffer[offset + 2] = r;
            }
        }
    }

    /// Write 16-bit RGB values to buffer in the correct order.
    pub fn write_rgb_u16(&self, buffer: &mut [u16], offset: usize, r: u16, g: u16, b: u16) {
        if offset + 3 > buffer.len() {
            return;
        }
        match self {
            ColorOrder::Rgb => {
                buffer[offset] = r;
                buffer[offset + 1] = g;
                buffer[offset + 2] = b;
            }
            ColorOrder::Grb => {
                buffer[offset] = g;
                buffer[offset + 1] = r;
                buffer[offset + 2] = b;
            }
            ColorOrder::Rbg => {
                buffer[offset] = r;
                buffer[offset + 1] = b;
                buffer[offset + 2] = g;
            }
            ColorOrder::Gbr => {
                buffer[offset] = g;
                buffer[offset + 1] = b;
                buffer[offset + 2] = r;
            }
            ColorOrder::Brg => {
                buffer[offset] = b;
                buffer[offset + 1] = r;
                buffer[offset + 2] = g;
            }
            ColorOrder::Bgr => {
                buffer[offset] = b;
                buffer[offset + 1] = g;
                buffer[offset + 2] = r;
            }
        }
    }
}

impl ToLpValue for ColorOrder {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for ColorOrder {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(&value)
                .ok_or_else(|| ValueRootError::new("expected RGB color order value")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ColorOrder {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ColorOrder");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("rgb", "RGB"),
                    SlotEnumOption::new("grb", "GRB"),
                    SlotEnumOption::new("rbg", "RBG"),
                    SlotEnumOption::new("gbr", "GBR"),
                    SlotEnumOption::new("brg", "BRG"),
                    SlotEnumOption::new("bgr", "BGR"),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;
    use crate::nodes::fixture::mapping::{PathSpec, RingOrder};
    use crate::{Affine2d, FixtureDefView, MapSlot, SlotPath, SlotShapeRegistry, StaticSlotShape};
    use alloc::collections::BTreeMap;

    #[test]
    fn test_fixture_def_kind() {
        let mut ring_lamp_counts = BTreeMap::new();
        ring_lamp_counts.insert(0, ValueSlot::new(1_u32));
        let mut paths = BTreeMap::new();
        paths.insert(
            0,
            EnumSlot::new(PathSpec::ring_array(
                [0.5, 0.5],
                1.0,
                0,
                1,
                MapSlot::new(ring_lamp_counts),
                0.0,
                RingOrder::InnerFirst,
            )),
        );
        let def = FixtureDef {
            render_size: default_render_size(),
            bindings: BindingDefs::default(),
            sampling: ValueSlot::new(FixtureSamplingConfig::TextureArea),
            mapping: EnumSlot::new(MappingConfig::path_points(MapSlot::new(paths), 2.0)),
            color_order: ValueSlot::new(ColorOrder::Rgb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: OptionSlot::none(),
            gamma_correction: OptionSlot::none(),
        };
        assert_eq!(def.kind(), NodeKind::Fixture);
    }

    #[test]
    fn test_color_order_as_str() {
        assert_eq!(ColorOrder::Rgb.as_str(), "rgb");
        assert_eq!(ColorOrder::Grb.as_str(), "grb");
        assert_eq!(ColorOrder::Bgr.as_str(), "bgr");
    }

    #[test]
    fn test_color_order_bytes_per_pixel() {
        assert_eq!(ColorOrder::Rgb.bytes_per_pixel(), 3);
        assert_eq!(ColorOrder::Grb.bytes_per_pixel(), 3);
    }

    #[test]
    fn test_color_order_write_rgb() {
        let mut buffer = [0u8; 10];

        ColorOrder::Rgb.write_rgb(&mut buffer, 0, 100, 200, 255);
        assert_eq!(buffer[0], 100);
        assert_eq!(buffer[1], 200);
        assert_eq!(buffer[2], 255);

        ColorOrder::Grb.write_rgb(&mut buffer, 3, 100, 200, 255);
        assert_eq!(buffer[3], 200);
        assert_eq!(buffer[4], 100);
        assert_eq!(buffer[5], 255);

        ColorOrder::Bgr.write_rgb(&mut buffer, 6, 100, 200, 255);
        assert_eq!(buffer[6], 255);
        assert_eq!(buffer[7], 200);
        assert_eq!(buffer[8], 100);
    }

    #[test]
    fn test_color_order_write_rgb_bounds_check() {
        let mut buffer = [0u8; 2];
        ColorOrder::Rgb.write_rgb(&mut buffer, 0, 100, 200, 255);
    }

    #[test]
    fn generated_fixture_def_view_compiles() {
        let mut registry = SlotShapeRegistry::default();
        FixtureDef::ensure_registered(&mut registry).expect("fixture shape");

        let view = FixtureDefView::compile(&registry).expect("fixture def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(
            view.render_size().path(),
            &SlotPath::parse("render_size").unwrap()
        );
        assert_eq!(
            view.color_order().path(),
            &SlotPath::parse("color_order").unwrap()
        );
        assert_eq!(
            view.brightness().path(),
            &SlotPath::parse("brightness").unwrap()
        );
        assert_eq!(
            view.gamma_correction().path(),
            &SlotPath::parse("gamma_correction").unwrap()
        );
    }
}
