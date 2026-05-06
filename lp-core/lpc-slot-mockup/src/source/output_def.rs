use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeChild, SlotShapeId,
    SlotShapeRegistry, SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, id, record, value, version};

pub struct OutputDef {
    pin: SlotValue<u32>,
    interpolate: SlotValue<bool>,
    dither: SlotValue<bool>,
}

impl OutputDef {
    pub fn new() -> Self {
        Self {
            pin: SlotValue::new(18),
            interpolate: SlotValue::new(true),
            dither: SlotValue::new(false),
        }
    }
}

impl Default for OutputDef {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotAccess for OutputDef {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for OutputDef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.output");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        use SlotShapeChild::Owned;
        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            vec![
                record(
                    "source.output",
                    vec![
                        field("pin", Owned(id("source.output.pin"))),
                        field("interpolate", Owned(id("source.output.interpolate"))),
                        field("dither", Owned(id("source.output.dither"))),
                    ],
                ),
                value("source.output.pin", ModelType::U32),
                value("source.output.interpolate", ModelType::Bool),
                value("source.output.dither", ModelType::Bool),
            ],
        )
    }
}

impl SlotRecordAccess for OutputDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.pin)),
            1 => Some(SlotDataAccess::Value(&self.interpolate)),
            2 => Some(SlotDataAccess::Value(&self.dither)),
            _ => None,
        }
    }
}
