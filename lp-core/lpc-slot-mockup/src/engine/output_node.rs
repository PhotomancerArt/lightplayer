use lpc_model::{ModelType, SlotValue};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "engine.output_node")]
pub struct OutputNode {
    #[slot(value = ModelType::U32)]
    frames_sent: SlotValue<u32>,
}

impl OutputNode {
    pub fn new() -> Self {
        Self {
            frames_sent: SlotValue::new(0),
        }
    }
}

impl Default for OutputNode {
    fn default() -> Self {
        Self::new()
    }
}
