use alloc::vec::Vec;

/// Replace or delete an artifact body.
///
/// Asset overlays are used for any whole-body artifact edit, including shader
/// source assets, fixture SVGs, and full node-definition artifact replacement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetOverlay {
    /// Delete the artifact body from the effective project.
    Delete,
    /// Replace the effective artifact body with these bytes.
    ReplaceBody(Vec<u8>),
}
