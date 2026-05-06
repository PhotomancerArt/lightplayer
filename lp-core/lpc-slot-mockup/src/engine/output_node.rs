use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeChild, SlotShapeId,
    SlotShapeRegistry, SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, id, record, value, version};

pub struct OutputNode {
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

impl SlotAccess for OutputNode {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for OutputNode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("engine.output_node");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        use SlotShapeChild::Owned;

        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            vec![
                record(
                    "engine.output_node",
                    vec![field(
                        "frames_sent",
                        Owned(id("engine.output_node.frames_sent")),
                    )],
                ),
                value("engine.output_node.frames_sent", ModelType::U32),
            ],
        )
    }
}

impl SlotRecordAccess for OutputNode {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.frames_sent)),
            _ => None,
        }
    }
}
