use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, record, value};

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
        registry.register_tree(
            Self::SHAPE_ID,
            record(vec![
                field("pin", value(ModelType::U32)),
                field("interpolate", value(ModelType::Bool)),
                field("dither", value(ModelType::Bool)),
            ]),
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
