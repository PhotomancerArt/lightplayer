//! Asset references discovered from node definitions.

use crate::{AssetKind, AssetSource};

/// One asset referenced by a node definition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferencedAsset {
    pub source: AssetSource,
    pub kind: AssetKind,
}

impl ReferencedAsset {
    pub fn new(source: AssetSource, kind: AssetKind) -> Self {
        Self { source, kind }
    }
}
