use lpc_model::{BindingDefs, Dim2u, Dim2uSlot, SlotRecord};

#[derive(SlotRecord)]
pub struct TextureDef {
    #[slot(skip)]
    pub kind: String,
    size: Dim2uSlot,
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

    pub fn from_codec(size: Dim2u) -> Self {
        Self {
            kind: Self::KIND.to_string(),
            size: Dim2uSlot::new(size),
            bindings: BindingDefs::default(),
        }
    }

    pub fn size(&self) -> Dim2u {
        *self.size.value()
    }

    pub fn bindings(&self) -> &BindingDefs {
        &self.bindings
    }
}

impl Default for TextureDef {
    fn default() -> Self {
        Self::new()
    }
}
