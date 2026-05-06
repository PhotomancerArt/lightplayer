use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, record, value};

pub struct TextureDef {
    width: SlotValue<u32>,
    height: SlotValue<u32>,
}

impl TextureDef {
    pub fn new() -> Self {
        Self {
            width: SlotValue::new(64),
            height: SlotValue::new(32),
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
            record(vec![
                field("width", value(ModelType::U32)),
                field("height", value(ModelType::U32)),
            ]),
        )
    }
}

impl SlotRecordAccess for TextureDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.width)),
            1 => Some(SlotDataAccess::Value(&self.height)),
            _ => None,
        }
    }
}
