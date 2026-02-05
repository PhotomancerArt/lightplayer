//! Project handle type for identifying loaded projects

use serde::{Deserialize, Serialize};

/// Handle for a loaded project
///
/// This is an opaque identifier used to reference a project that has been loaded
/// on the server. The server maintains a mapping from handles to project instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectHandle(pub u32);

impl ProjectHandle {
    /// Create a new project handle with the given ID
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the underlying ID
    pub fn id(&self) -> u32 {
        self.0
    }

    /// Get the underlying ID as i32
    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}
