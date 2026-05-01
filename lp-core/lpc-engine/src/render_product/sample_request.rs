//! Batch sampling request types (fixture-oriented coordinates).

use alloc::vec::Vec;

/// Normalized or surface-space sample coordinate; exact interpretation is product-defined.
#[derive(Debug, Clone)]
pub struct RenderSamplePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct RenderSampleBatch {
    pub points: Vec<RenderSamplePoint>,
}
