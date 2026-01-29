use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MappingConfig {
    /// A mapping defined by an svg-like path with equally spaced leds along the path
    PathPoints {
        paths: Vec<PathSpec>,

        // Diameter of the sampling circle in texture pixels
        // 1 means only the center pixel is sampled
        // 2 means the center pixel and roughly the 8 surrounding pixels are sampled
        sample_diameter: f32,
    },
}

/// Specifies paths for a single fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PathSpec {
    /// A display made of concentric rings of lamps, usually LEDs on a PCB
    RingArray {
        /// Center of the display in texture space [0, 1]
        center: (f32, f32),
        /// Diameter of the display in texture space [0, 1]
        diameter: f32,
        /// Start ring index. 0 is the center ring.
        start_ring_inclusive: u32,
        /// End ring, exclusive
        end_ring_exclusive: u32,
        /// Number of lamps in each ring
        ring_lamp_counts: Vec<u32>,
        /// Offset angle in radians
        offset_angle: f32,
        order: RingOrder,
    },
    // Later expansion, a path defined by an svg-like path with equally spaced leds along the path
    // SvgPath {
    //     svg_path: String,
    //     lamp_count: u32,
    // },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RingOrder {
    InnerFirst,
    OuterFirst,
    // Could do a custom order later.
}
