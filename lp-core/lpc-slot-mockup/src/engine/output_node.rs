use lpc_model::ValueSlot;

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
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
