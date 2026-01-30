use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::nodes::handle::NodeHandle;

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureState {
    /// Lamp color values (RGB per lamp)
    pub lamp_colors: Vec<u8>,
    /// Post-transform mapping cells (sampling regions)
    pub mapping_cells: Vec<MappingCell>,
    /// Resolved texture handle (if fixture has been initialized)
    pub texture_handle: Option<NodeHandle>,
    /// Resolved output handle (if fixture has been initialized)
    pub output_handle: Option<NodeHandle>,
}
