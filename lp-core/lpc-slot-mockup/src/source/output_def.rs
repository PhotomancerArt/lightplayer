use lpc_model::{ModelType, SlotValue};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.output")]
pub struct OutputDef {
    #[slot(value = ModelType::U32)]
    pin: SlotValue<u32>,
    #[slot(value = ModelType::Bool)]
    interpolate: SlotValue<bool>,
    #[slot(value = ModelType::Bool)]
    dither: SlotValue<bool>,
}

impl OutputDef {
    pub fn new() -> Self {
        Self {
            pin: SlotValue::new(18),
            interpolate: SlotValue::new(true),
            dither: SlotValue::new(false),
        }
    }
}

impl Default for OutputDef {
    fn default() -> Self {
        Self::new()
    }
}
