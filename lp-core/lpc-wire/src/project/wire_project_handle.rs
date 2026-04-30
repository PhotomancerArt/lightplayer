//! Opaque wire token identifying a loaded project on the server.

use serde::{Deserialize, Serialize};

/// Handle for a loaded project (wire-visible opaque id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WireProjectHandle(pub u32);

impl WireProjectHandle {
    /// Create a new project handle with the given ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Underlying numeric id.
    pub fn id(&self) -> u32 {
        self.0
    }

    /// Same id as signed `i32` for legacy call sites.
    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}
