use lpc_model::ValueSlot;

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct OutputDef {
    pin: ValueSlot<u32>,
    interpolate: ValueSlot<bool>,
    dither: ValueSlot<bool>,
}

impl OutputDef {
    pub fn new() -> Self {
        Self {
            pin: ValueSlot::new(18),
            interpolate: ValueSlot::new(true),
            dither: ValueSlot::new(false),
        }
    }
}

impl Default for OutputDef {
    fn default() -> Self {
        Self::new()
    }
}
