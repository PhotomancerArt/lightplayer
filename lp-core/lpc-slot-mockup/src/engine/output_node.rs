use lpc_model::{SlotRecord, ValueSlot};

#[derive(SlotRecord)]
pub struct OutputNode {
    frames_sent: ValueSlot<u32>,
}

impl OutputNode {
    pub fn new() -> Self {
        Self {
            frames_sent: ValueSlot::new(0),
        }
    }
}

impl Default for OutputNode {
    fn default() -> Self {
        Self::new()
    }
}
