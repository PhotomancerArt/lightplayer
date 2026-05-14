use serde::{Deserialize, Serialize};

use crate::{BindingDefs, OptionSlot, PositiveF32Slot, RatioSlot, SlotRecord, ValueSlot};

/// Authored GPIO output node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SlotRecord)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
    /// Authored slot bindings for output inputs.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    /// Optional display pipeline options.
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    pub fn new(pin: u32) -> Self {
        Self {
            pin: ValueSlot::new(pin),
            bindings: BindingDefs::default(),
            options: OptionSlot::none(),
        }
    }

    pub fn pin(&self) -> u32 {
        *self.pin.value()
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Output
    }

    pub fn options(&self) -> Option<&OutputDriverOptionsConfig> {
        self.options.data.as_ref()
    }
}

/// Authored output driver options for the display pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SlotRecord)]
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
    use crate::{OutputDefView, SlotPath, SlotShapeRegistry, StaticSlotShape};

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

    #[test]
    fn generated_output_def_view_compiles() {
        let mut registry = SlotShapeRegistry::default();
        OutputDef::ensure_registered(&mut registry).expect("output shape");

        let view = OutputDefView::compile(&registry).expect("output def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(view.pin().path(), &SlotPath::parse("pin").unwrap());
        assert_eq!(view.options().path(), &SlotPath::parse("options").unwrap());
    }
}
