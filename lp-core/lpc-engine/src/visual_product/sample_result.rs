//! Batch sampling result types.

use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct VisualSample {
    pub rgba_unorm16: [u16; 4],
}

#[derive(Debug, Clone)]
pub struct VisualSampleBatchResult {
    pub samples: Vec<VisualSample>,
}
