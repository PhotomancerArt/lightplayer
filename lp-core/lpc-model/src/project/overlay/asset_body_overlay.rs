use alloc::vec::Vec;

/// Replace or delete an artifact body.
///
/// Asset overlays are used for any whole-body artifact edit, including shader
/// source assets, fixture SVGs, and full node-definition artifact replacement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetBodyOverlay {
    /// Delete the artifact body from the effective project.
    Delete,
    /// Replace the effective artifact body with these bytes.
    ///
    /// On the wire the body is text when it is UTF-8 (every editor's body),
    /// else `{"base64":"…"}` ([`crate::body_bytes`]) — never serde's array
    /// of numbers, ~3.5 characters a byte, which outgrew the board's message
    /// cap past ~4.7 KB of shader (wire 41).
    ReplaceBody(#[serde(with = "crate::body_bytes")] Vec<u8>),
}
