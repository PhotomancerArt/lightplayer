use crate::serde_base64;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Output node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputState {
    /// Channel data buffer
    #[serde(
        serialize_with = "serde_base64::serialize",
        deserialize_with = "serde_base64::deserialize"
    )]
    pub channel_data: Vec<u8>,
}
