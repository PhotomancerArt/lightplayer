use std::collections::BTreeMap;

use crate::source::MappingConfig;
use lpc_model::{MapSlot, PositiveF32, PositiveF32Slot, SlotRecord, Xy, XySlot};

#[derive(SlotRecord)]
pub struct FixtureNode {
    pub touches: MapSlot<u32, TouchState>,
    pub mapping_preview: MappingConfig,
}

#[derive(SlotRecord)]
pub struct TouchState {
    pub position: XySlot,
    pub pressure: PositiveF32Slot,
}

impl FixtureNode {
    pub fn new() -> Self {
        let mut touches = BTreeMap::new();
        touches.insert(1, TouchState::new([0.2, 0.3], 0.7));
        touches.insert(2, TouchState::new([0.8, 0.4], 0.4));

        Self {
            touches: MapSlot::new(touches),
            mapping_preview: MappingConfig::path_points_default(),
        }
    }
    pub fn switch_mapping_preview(&mut self) {
        self.mapping_preview = MappingConfig::square();
    }

    pub fn disable_mapping_preview(&mut self) {
        self.mapping_preview = MappingConfig::disabled();
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
            position: XySlot::new(Xy(position)),
            pressure: PositiveF32Slot::new(PositiveF32(pressure)),
        }
    }
}

impl Default for TouchState {
    fn default() -> Self {
        Self::new([0.0, 0.0], 1.0)
    }
}
