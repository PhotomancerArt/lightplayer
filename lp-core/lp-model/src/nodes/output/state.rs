use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Output node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputState {
    /// Channel data buffer
    pub channel_data: Vec<u8>,
}
