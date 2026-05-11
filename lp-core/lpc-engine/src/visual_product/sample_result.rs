//! Batch sampling result types.

use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct VisualSample {
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct VisualSampleBatchResult {
    pub samples: Vec<VisualSample>,
}
