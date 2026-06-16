use crate::{AssetContentType, AssetLocation};

/// One asset referenced by a node definition.
///
/// Node definition topology APIs return `ReferencedAsset` values while walking
/// authored definitions. The registry turns those references into
/// [`crate::AssetEntry`] records in the effective project inventory.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferencedAsset {
    /// Identity of the referenced asset.
    pub location: AssetLocation,
    /// Coarse content type expected by the referring node definition.
    pub content_type: AssetContentType,
}

impl ReferencedAsset {
    pub fn new(location: AssetLocation, content_type: AssetContentType) -> Self {
        Self {
            location,
            content_type,
        }
    }
}
