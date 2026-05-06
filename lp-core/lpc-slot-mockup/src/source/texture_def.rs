use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeChild, SlotShapeId,
    SlotShapeRegistry, SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, id, record, value, version};

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
        use SlotShapeChild::Owned;
        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            vec![
                record(
                    "source.texture",
                    vec![
                        field("width", Owned(id("source.texture.width"))),
                        field("height", Owned(id("source.texture.height"))),
                    ],
                ),
                value("source.texture.width", ModelType::U32),
                value("source.texture.height", ModelType::U32),
            ],
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
