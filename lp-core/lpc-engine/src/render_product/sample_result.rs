//! Batch sampling result types.

use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct RenderSample {
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct RenderSampleBatchResult {
    pub samples: Vec<RenderSample>,
}
