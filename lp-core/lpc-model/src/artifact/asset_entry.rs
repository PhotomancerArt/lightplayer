//! Effective project asset inventory entry.

use crate::{ArtifactLocation, AssetState, Revision};

/// One referenced non-definition artifact in the effective project inventory.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetEntry {
    pub location: ArtifactLocation,
    pub state: AssetState,
    pub revision: Revision,
}

impl AssetEntry {
    pub fn new(location: ArtifactLocation, state: AssetState, revision: Revision) -> Self {
        Self {
            location,
            state,
            revision,
        }
    }
}
