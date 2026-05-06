use lpc_model::{
    Dim2u, Dim2uSlot, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, StaticSlotAccess, dim2u_shape,
};

use crate::model::{field, leaf, record};

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

impl SlotAccess for TextureDef {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for TextureDef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.texture");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        registry.register_tree(
            Self::SHAPE_ID,
            record(vec![field("size", leaf(dim2u_shape()))]),
        )
    }
}

impl SlotRecordAccess for TextureDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.size)),
            _ => None,
        }
    }
}
