//! Batch sampling request types (fixture-oriented coordinates).

use alloc::vec::Vec;

/// Integer texel coordinate in the materialized visual surface.
#[derive(Debug, Clone)]
pub struct VisualSamplePoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub struct VisualSampleBatch {
    pub points: Vec<VisualSamplePoint>,
}
