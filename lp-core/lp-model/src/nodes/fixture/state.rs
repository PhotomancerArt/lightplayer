use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::nodes::handle::NodeHandle;
use crate::project::FrameId;
use crate::state::StateField;

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
}

/// Wrapper for serializing FixtureState with a since_frame context
pub struct SerializableFixtureState<'a> {
    state: &'a FixtureState,
    since_frame: FrameId,
}

impl<'a> SerializableFixtureState<'a> {
    pub fn new(state: &'a FixtureState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl<'a> Serialize for SerializableFixtureState<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("FixtureState", 4)?;

        if is_initial_sync || self.state.lamp_colors.changed_frame() > self.since_frame {
            state.serialize_field("lamp_colors", self.state.lamp_colors.value())?;
        }
        if is_initial_sync || self.state.mapping_cells.changed_frame() > self.since_frame {
            state.serialize_field("mapping_cells", self.state.mapping_cells.value())?;
        }
        if is_initial_sync || self.state.texture_handle.changed_frame() > self.since_frame {
            state.serialize_field("texture_handle", self.state.texture_handle.value())?;
        }
        if is_initial_sync || self.state.output_handle.changed_frame() > self.since_frame {
            state.serialize_field("output_handle", self.state.output_handle.value())?;
        }

        state.end()
    }
}

// Temporary: Simple Serialize implementation for NodeState compatibility
// This will be replaced with proper context-aware serialization in Phase 8
impl Serialize for FixtureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FixtureState", 4)?;
        state.serialize_field("lamp_colors", self.lamp_colors.value())?;
        state.serialize_field("mapping_cells", self.mapping_cells.value())?;
        state.serialize_field("texture_handle", self.texture_handle.value())?;
        state.serialize_field("output_handle", self.output_handle.value())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for FixtureState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FixtureStateHelper {
            lamp_colors: Option<Vec<u8>>,
            mapping_cells: Option<Vec<MappingCell>>,
            texture_handle: Option<Option<NodeHandle>>,
            output_handle: Option<Option<NodeHandle>>,
        }

        let helper = FixtureStateHelper::deserialize(deserializer)?;

        // For deserialization, we need a frame_id, but we don't have one in JSON
        // Use default - this will be updated when merged with existing state
        let frame_id = FrameId::default();

        let mut state = FixtureState::new(frame_id);

        if let Some(val) = helper.lamp_colors {
            state.lamp_colors.set(frame_id, val);
        }
        if let Some(val) = helper.mapping_cells {
            state.mapping_cells.set(frame_id, val);
        }
        if let Some(val) = helper.texture_handle {
            state.texture_handle.set(frame_id, val);
        }
        if let Some(val) = helper.output_handle {
            state.output_handle.set(frame_id, val);
        }

        Ok(state)
    }
}
