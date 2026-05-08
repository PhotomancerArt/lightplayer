use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::node::kind::NodeKind;
use crate::node::node_def::NodeDef;
use crate::nodes::fixture::MappingConfig;
use crate::{
    Affine2dSlot, BindingDefs, FromLpValue, LpValue, OptionSlot, RelativeNodeRef,
    RelativeNodeRefSlot, SlotShapeId, SlotValue, SlotValueShape, ToLpValue, ValueRootError,
    ValueSlot,
};

/// Authored fixture node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct FixtureDef {
    /// Output node locator.
    pub output_loc: RelativeNodeRefSlot,
    /// Authored slot bindings for fixture inputs.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    /// Fixture mapping definition.
    #[slot(enum)]
    pub mapping: MappingConfig,
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
    pub fn output_loc(&self) -> &RelativeNodeRef {
        self.output_loc.value()
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
}

fn default_brightness() -> OptionSlot<ValueSlot<u32>> {
    OptionSlot::some(ValueSlot::new(64))
}

fn default_gamma_correction() -> OptionSlot<ValueSlot<bool>> {
    OptionSlot::some(ValueSlot::new(true))
}

impl NodeDef for FixtureDef {
    fn kind(&self) -> NodeKind {
        NodeKind::Fixture
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// Color order for RGB channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorOrder {
    /// Red, Green, Blue.
    Rgb,
    /// Green, Red, Blue.
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
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
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
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.color_order");

    fn value_shape() -> SlotValueShape {
        crate::color_order_shape()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use crate::nodes::fixture::mapping::{PathSpec, RingOrder};

    #[test]
    fn test_fixture_def_kind() {
        let mut ring_lamp_counts = BTreeMap::new();
        ring_lamp_counts.insert(0, ValueSlot::new(1));
        let mut paths = BTreeMap::new();
        paths.insert(
            0,
            PathSpec::ring_array(
                [0.5, 0.5],
                1.0,
                0,
                1,
                MapSlot::new(ring_lamp_counts),
                0.0,
                RingOrder::InnerFirst,
            ),
        );
        let def = FixtureDef {
            output_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..out_output").unwrap()),
            bindings: BindingDefs::default(),
            mapping: MappingConfig::path_points(MapSlot::new(paths), 2.0),
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
}
