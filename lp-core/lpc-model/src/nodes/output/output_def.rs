use serde::{Deserialize, Serialize};

use crate::node::kind::NodeKind;
use crate::node::node_def::NodeDef;
use crate::{OptionSlot, PositiveF32Slot, RatioSlot, ValueSlot};

/// Authored GPIO output node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
    /// Optional display pipeline options.
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}

impl OutputDef {
    pub fn new(pin: u32) -> Self {
        Self {
            pin: ValueSlot::new(pin),
            options: OptionSlot::none(),
        }
    }

    pub fn pin(&self) -> u32 {
        *self.pin.value()
    }

    pub fn options(&self) -> Option<&OutputDriverOptionsConfig> {
        self.options.data.as_ref()
    }
}

impl NodeDef for OutputDef {
    fn kind(&self) -> NodeKind {
        NodeKind::Output
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// Authored output driver options for the display pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
pub struct OutputDriverOptionsConfig {
    /// Gamma exponent for luminance curve.
    #[serde(default = "default_lum_power_slot")]
    pub lum_power: PositiveF32Slot,
    /// RGB white point balance.
    #[serde(default = "default_white_point_slot")]
    pub white_point: ValueSlot<[f32; 3]>,
    /// Global brightness multiplier.
    #[serde(default = "default_brightness_slot")]
    pub brightness: RatioSlot,
    /// Enable interpolation between frames.
    #[serde(default = "default_true_slot")]
    pub interpolation_enabled: ValueSlot<bool>,
    /// Enable temporal dithering.
    #[serde(default = "default_true_slot")]
    pub dithering_enabled: ValueSlot<bool>,
    /// Enable gamma + white point LUT.
    #[serde(default = "default_true_slot")]
    pub lut_enabled: ValueSlot<bool>,
}

impl Default for OutputDriverOptionsConfig {
    fn default() -> Self {
        Self {
            lum_power: default_lum_power_slot(),
            white_point: default_white_point_slot(),
            brightness: default_brightness_slot(),
            interpolation_enabled: default_true_slot(),
            dithering_enabled: default_true_slot(),
            lut_enabled: default_true_slot(),
        }
    }
}

fn default_lum_power_slot() -> PositiveF32Slot {
    PositiveF32Slot::new(2.0)
}

fn default_white_point_slot() -> ValueSlot<[f32; 3]> {
    ValueSlot::new([0.9, 1.0, 1.0])
}

fn default_brightness_slot() -> RatioSlot {
    RatioSlot::new(1.0)
}

fn default_true_slot() -> ValueSlot<bool> {
    ValueSlot::new(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::kind::NodeKind;

    #[test]
    fn test_output_def_kind() {
        let def = OutputDef::new(18);
        assert_eq!(def.kind(), NodeKind::Output);
        assert_eq!(def.pin(), 18);
    }

    #[test]
    fn test_output_def_flat_toml_deserialize() {
        let toml = r#"
kind = "output"
pin = 18

[options]
brightness = 0.25
dithering_enabled = false
"#;
        let def: OutputDef = toml::from_str(toml).unwrap();
        assert_eq!(def.pin(), 18);
        let opts = def.options().unwrap();
        assert!((*opts.brightness.value() - 0.25).abs() < 0.001);
        assert!(!*opts.dithering_enabled.value());
        assert!(*opts.interpolation_enabled.value());
    }
}
