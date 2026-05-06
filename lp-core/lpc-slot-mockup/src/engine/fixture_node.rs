use std::collections::BTreeMap;

use crate::source::FixtureMapping;
use lpc_model::{PositiveF32Slot, SlotMap, XySlot, positive_f32_shape, xy_shape};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "engine.fixture_node")]
pub struct FixtureNode {
    #[slot(map(key = "u32", value_ref = "engine.touch"))]
    touches: SlotMap<u32, TouchState>,
    #[slot(enum)]
    mapping_preview: FixtureMapping,
}

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "engine.touch")]
pub struct TouchState {
    #[slot(leaf = xy_shape())]
    position: XySlot,
    #[slot(leaf = positive_f32_shape())]
    pressure: PositiveF32Slot,
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

    pub fn disable_mapping_preview(&mut self) {
        self.mapping_preview = FixtureMapping::disabled();
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

impl TouchState {
    fn new(position: [f32; 2], pressure: f32) -> Self {
        Self {
            position: XySlot::new(position),
            pressure: PositiveF32Slot::new(pressure),
        }
    }
}
