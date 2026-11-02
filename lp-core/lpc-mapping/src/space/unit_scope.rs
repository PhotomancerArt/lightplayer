//! `unit`'s three uniforms, defined through T (theory D22).

use crate::space::space_transform::SpaceTransform;

/// What a `unit` shader learns about the fixture it is rendering
/// (`patternExtent`, `patternPitch`, `lampCount`), each through the fit's T:
/// the fixture's box as the shader sees it, not the lamps' (D22). Cover on a
/// 3:1 box gives an extent of (1, 1/3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitScope {
    /// Half-size of the fixture's box in source units (its image through T,
    /// axis by axis).
    pub extent: [f32; 2],
    /// The lamp pitch in source units.
    pub pitch: f32,
    /// Number of lamps.
    pub lamp_count: u32,
}

impl UnitScope {
    /// The scope of a fixture whose lamps are `pitch` apart in its own units
    /// and number `lamp_count`, as `transform` shows them.
    #[must_use]
    pub fn new(transform: &SpaceTransform, pitch: f32, lamp_count: u32) -> Self {
        Self {
            extent: transform.box_half_extent(),
            pitch: (f64::from(pitch) * transform.length_scale()) as f32,
            lamp_count,
        }
    }
}
