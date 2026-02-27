use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::nodes::handle::NodeHandle;
use crate::project::FrameId;
use crate::state::StateField;

use crate::impl_state_serialization;

/// Mapping cell - represents a post-transform sampling region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingCell {
    /// Output channel index
    pub channel: u32,
    /// Center coordinates in texture space [0, 1] (post-transform)
    pub center: [f32; 2],
    /// Sampling radius
    pub radius: f32,
}

/// Fixture node state - runtime values
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureState {
    /// Lamp color values (RGB per lamp)
    pub lamp_colors: StateField<Vec<u8>>,
    /// Post-transform mapping cells (sampling regions)
    pub mapping_cells: StateField<Vec<MappingCell>>,
    /// Resolved texture handle (if fixture has been initialized)
    pub texture_handle: StateField<Option<NodeHandle>>,
    /// Resolved output handle (if fixture has been initialized)
    pub output_handle: StateField<Option<NodeHandle>>,
}

impl FixtureState {
    /// Create a new FixtureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            lamp_colors: StateField::new(frame_id, Vec::new()),
            mapping_cells: StateField::new(frame_id, Vec::new()),
            texture_handle: StateField::new(frame_id, None),
            output_handle: StateField::new(frame_id, None),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        // Merge lamp_colors if present (not empty)
        if !other.lamp_colors.value().is_empty() {
            self.lamp_colors
                .set(frame_id, other.lamp_colors.value().clone());
        }
        // Merge mapping_cells if present (not empty)
        if !other.mapping_cells.value().is_empty() {
            self.mapping_cells
                .set(frame_id, other.mapping_cells.value().clone());
        }
        // Merge texture_handle if present (Some value)
        if other.texture_handle.value().is_some() {
            self.texture_handle
                .set(frame_id, *other.texture_handle.value());
        }
        // Merge output_handle if present (Some value)
        if other.output_handle.value().is_some() {
            self.output_handle
                .set(frame_id, *other.output_handle.value());
        }
    }
}

impl_state_serialization! {
    FixtureState => SerializableFixtureState {
        lamp_colors: Vec<u8>,
        mapping_cells: Vec<MappingCell>,
        texture_handle: Option<NodeHandle>,
        output_handle: Option<NodeHandle>,
    }
}
