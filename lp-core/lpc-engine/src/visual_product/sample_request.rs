//! Batch sampling request types (fixture-oriented coordinates).

use alloc::vec::Vec;

/// Normalized or surface-space sample coordinate; exact interpretation is product-defined.
#[derive(Debug, Clone)]
pub struct VisualSamplePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct VisualSampleBatch {
    pub points: Vec<VisualSamplePoint>,
}
