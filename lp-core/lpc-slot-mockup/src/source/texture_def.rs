use lpc_model::{BindingDefs, Dim2u, Dim2uSlot, SlotRecord};

#[derive(Default, SlotRecord)]
pub struct TextureDef {
    pub size: Dim2uSlot,
    pub bindings: BindingDefs,
}

impl TextureDef {
    pub const KIND: &'static str = "texture";

    pub fn new() -> Self {
        Self {
            size: Dim2uSlot::new(Dim2u {
                width: 64,
                height: 32,
            }),
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
