use crate::{AssetKind, AssetSource};

/// One asset referenced by a node definition.
///
/// Node definition topology APIs return `AssetRef` values while walking
/// authored definitions. The registry turns those references into
/// [`crate::AssetEntry`] records in the effective project inventory.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetRef {
    /// Identity of the referenced asset.
    pub source: AssetSource,
    /// Coarse kind expected by the referring node definition.
    pub kind: AssetKind,
}

impl AssetRef {
    pub fn new(source: AssetSource, kind: AssetKind) -> Self {
        Self { source, kind }
    }
}
