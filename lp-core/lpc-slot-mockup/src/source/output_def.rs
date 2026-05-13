use lpc_model::{BindingDefs, OptionSlot, PositiveF32Slot, RatioSlot, ValueSlot};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct OutputDef {
    #[slot(skip)]
    pub kind: String,
    pin: ValueSlot<u32>,
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    bindings: BindingDefs,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    options: OptionSlot<OutputDriverOptionsConfig>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
pub struct OutputDriverOptionsConfig {
    #[serde(default = "default_lum_power_slot")]
    lum_power: PositiveF32Slot,
    #[serde(default = "default_white_point_slot")]
    white_point: ValueSlot<[f32; 3]>,
    #[serde(default = "default_brightness_slot")]
    brightness: RatioSlot,
    #[serde(default = "default_true_slot")]
    interpolation_enabled: ValueSlot<bool>,
    #[serde(default = "default_true_slot")]
    dithering_enabled: ValueSlot<bool>,
    #[serde(default = "default_true_slot")]
    lut_enabled: ValueSlot<bool>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    pub fn new() -> Self {
        Self {
            kind: Self::KIND.to_string(),
            pin: ValueSlot::new(18),
            bindings: BindingDefs::default(),
            options: OptionSlot::some(OutputDriverOptionsConfig::default()),
        }
    }
}

impl Default for OutputDef {
    fn default() -> Self {
        Self::new()
    }
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
