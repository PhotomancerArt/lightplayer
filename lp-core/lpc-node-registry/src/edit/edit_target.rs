//! Artifact addressing for pending and committed files.

use lpfs::LpPathBuf;

use crate::ArtifactLocation;

/// Target file for an [`super::ArtifactEdit`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditTarget {
    /// Registered artifact (`file:/…` URI on wire).
    Location(ArtifactLocation),
    /// Absolute project path — primary authoring form; implicit slot overlay create.
    Path(LpPathBuf),
}
