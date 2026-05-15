use lpc_model::{
    BindingDefs, OptionSlot, PositiveF32, PositiveF32Slot, Ratio, RatioSlot, SlotRecord, ValueSlot,
};

#[derive(SlotRecord)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
    pub bindings: BindingDefs,
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}

#[derive(Clone, Debug, PartialEq, SlotRecord)]
pub struct OutputDriverOptionsConfig {
    pub lum_power: PositiveF32Slot,
    pub white_point: ValueSlot<[f32; 3]>,
    pub brightness: RatioSlot,
    pub interpolation_enabled: ValueSlot<bool>,
    pub dithering_enabled: ValueSlot<bool>,
    pub lut_enabled: ValueSlot<bool>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    pub fn new() -> Self {
        Self {
            pin: ValueSlot::new(18),
            bindings: BindingDefs::default(),
            options: OptionSlot::some(OutputDriverOptionsConfig::default()),
        }
    }

    pub fn pin(&self) -> u32 {
        *self.pin.value()
    }

    pub fn options(&self) -> Option<&OutputDriverOptionsConfig> {
        self.options.data.as_ref()
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

impl OutputDriverOptionsConfig {
    pub fn lum_power(&self) -> f32 {
        self.lum_power.value().0
    }

    pub fn white_point(&self) -> [f32; 3] {
        *self.white_point.value()
    }

    pub fn brightness(&self) -> f32 {
        self.brightness.value().0
    }

    pub fn interpolation_enabled(&self) -> bool {
        *self.interpolation_enabled.value()
    }

    pub fn dithering_enabled(&self) -> bool {
        *self.dithering_enabled.value()
    }

    pub fn lut_enabled(&self) -> bool {
        *self.lut_enabled.value()
    }
}

fn default_lum_power_slot() -> PositiveF32Slot {
    PositiveF32Slot::new(PositiveF32(2.0))
}

fn default_white_point_slot() -> ValueSlot<[f32; 3]> {
    ValueSlot::new([0.9, 1.0, 1.0])
}

fn default_brightness_slot() -> RatioSlot {
    RatioSlot::new(Ratio(1.0))
}

fn default_true_slot() -> ValueSlot<bool> {
    ValueSlot::new(true)
}
