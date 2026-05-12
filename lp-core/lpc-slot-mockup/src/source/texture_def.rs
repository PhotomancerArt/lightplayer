use lpc_model::{Dim2u, Dim2uSlot};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct TextureDef {
    size: Dim2uSlot,
}

impl TextureDef {
    pub fn new() -> Self {
        Self {
            size: Dim2uSlot::new(Dim2u {
                width: 64,
                height: 32,
            }),
        }
    }
}

impl Default for TextureDef {
    fn default() -> Self {
        Self::new()
    }
}
