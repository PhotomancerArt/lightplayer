//! Effective project asset inventory entry.

use crate::{AssetKind, AssetSource, AssetState, Revision};

/// One referenced asset in the effective project inventory.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetEntry {
    pub source: AssetSource,
    pub kind: AssetKind,
    pub state: AssetState,
    pub revision: Revision,
}

impl AssetEntry {
    pub fn new(
        source: AssetSource,
        kind: AssetKind,
        state: AssetState,
        revision: Revision,
    ) -> Self {
        Self {
            source,
            kind,
            state,
            revision,
        }
    }
}
