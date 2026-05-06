use std::collections::BTreeMap;

use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotMap, SlotMapKeyShape, SlotMapValueAccess,
    SlotRecordAccess, SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError, SlotValue,
    StaticSlotAccess,
};

use crate::model::{field, id, map, mapping_shape, record, reference, value, version};
use crate::source::FixtureMapping;

pub struct FixtureNode {
    touches: SlotMap<u32, TouchState>,
    mapping_preview: FixtureMapping,
}

pub struct TouchState {
    position: SlotValue<[f32; 2]>,
    pressure: SlotValue<f32>,
}

impl FixtureNode {
    pub fn new() -> Self {
        let mut touches = BTreeMap::new();
        touches.insert(1, TouchState::new([0.2, 0.3], 0.7));
        touches.insert(2, TouchState::new([0.8, 0.4], 0.4));

        Self {
            touches: SlotMap::new(touches),
            mapping_preview: FixtureMapping::circle(),
        }
    }
    pub fn switch_mapping_preview(&mut self) {
        self.mapping_preview = FixtureMapping::square();
    }

    pub fn remove_touch(&mut self, id: u32) {
        self.touches.remove(&id);
    }
}

impl Default for FixtureNode {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotAccess for FixtureNode {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for FixtureNode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("engine.fixture_node");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        registry.register_tree(
            version(),
            id("engine.touch"),
            record(vec![
                field("position", value(ModelType::Vec2)),
                field("pressure", value(ModelType::F32)),
            ]),
        )?;

        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            record(vec![
                field(
                    "touches",
                    map(SlotMapKeyShape::U32, reference(id("engine.touch"))),
                ),
                field("mapping_preview", mapping_shape()),
            ]),
        )
    }
}

impl SlotRecordAccess for FixtureNode {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Map(&self.touches)),
            1 => Some(SlotDataAccess::Enum(&self.mapping_preview)),
            _ => None,
        }
    }
}

impl TouchState {
    fn new(position: [f32; 2], pressure: f32) -> Self {
        Self {
            position: SlotValue::new(position),
            pressure: SlotValue::new(pressure),
        }
    }
}

impl SlotMapValueAccess for TouchState {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for TouchState {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.position)),
            1 => Some(SlotDataAccess::Value(&self.pressure)),
            _ => None,
        }
    }
}
