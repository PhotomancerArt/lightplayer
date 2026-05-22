//! Path-level file body edits for opaque artifacts.

use alloc::string::String;

/// One file-body edit within an [`super::ArtifactEdit::Asset`] block.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetEdit {
    /// Remove this path on commit.
    Delete,
    /// Replace the whole file body — GLSL, SVG, etc.; optional TOML import escape hatch.
    ReplaceBody(String),
}

impl AssetEdit {
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::ReplaceBody(_) => "replace_body",
        }
    }
}
