use lpc_model::{BindingDefs, OptionSlot, PositiveF32Slot, RatioSlot, ValueSlot};

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct OutputDef {
    #[slot(skip)]
    pub kind: String,
    pin: ValueSlot<u32>,
    bindings: BindingDefs,
    options: OptionSlot<OutputDriverOptionsConfig>,
}

#[derive(Clone, Debug, PartialEq, lpc_model::SlotRecord)]
pub struct OutputDriverOptionsConfig {
    lum_power: PositiveF32Slot,
    white_point: ValueSlot<[f32; 3]>,
    brightness: RatioSlot,
    interpolation_enabled: ValueSlot<bool>,
    dithering_enabled: ValueSlot<bool>,
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

    pub fn from_codec(pin: u32, options: Option<OutputDriverOptionsConfig>) -> Self {
        Self {
            kind: Self::KIND.to_string(),
            pin: ValueSlot::new(pin),
            bindings: BindingDefs::default(),
            options: match options {
                Some(options) => OptionSlot::some(options),
                None => OptionSlot::none(),
            },
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
    pub fn from_codec(
        lum_power: f32,
        white_point: [f32; 3],
        brightness: f32,
        interpolation_enabled: bool,
        dithering_enabled: bool,
        lut_enabled: bool,
    ) -> Self {
        Self {
            lum_power: PositiveF32Slot::new(lum_power),
            white_point: ValueSlot::new(white_point),
            brightness: RatioSlot::new(brightness),
            interpolation_enabled: ValueSlot::new(interpolation_enabled),
            dithering_enabled: ValueSlot::new(dithering_enabled),
            lut_enabled: ValueSlot::new(lut_enabled),
        }
    }

    pub fn lum_power(&self) -> f32 {
        *self.lum_power.value()
    }

    pub fn white_point(&self) -> [f32; 3] {
        *self.white_point.value()
    }

    pub fn brightness(&self) -> f32 {
        *self.brightness.value()
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
