use crate::{AssetKind, AssetSource, AssetState, Revision};

/// One referenced asset in the effective project inventory.
///
/// This is the per-asset record stored in [`crate::ProjectInventory::assets`].
/// It keeps asset identity, expected kind, effective availability, and the
/// revision of the body or owning inline definition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetEntry {
    /// Stable identity for the referenced asset.
    pub source: AssetSource,
    /// Coarse specialization used by registry/engine consumers.
    pub kind: AssetKind,
    /// Effective availability after artifact state and overlay edits.
    pub state: AssetState,
    /// Revision of the effective asset body or owning inline definition.
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
