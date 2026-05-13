use lpc_model::{BindingDefs, Dim2u, Dim2uSlot};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct TextureDef {
    #[slot(skip)]
    pub kind: String,
    size: Dim2uSlot,
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    bindings: BindingDefs,
}

impl TextureDef {
    pub const KIND: &'static str = "texture";

    pub fn new() -> Self {
        Self {
            kind: Self::KIND.to_string(),
            size: Dim2uSlot::new(Dim2u {
                width: 64,
                height: 32,
            }),
            bindings: BindingDefs::default(),
        }
    }
}

impl Default for TextureDef {
    fn default() -> Self {
        Self::new()
    }
}
