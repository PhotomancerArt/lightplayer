//! Artifact addressing for pending and committed files.

use lpfs::LpPathBuf;

use crate::ArtifactLoc;

/// Target file for an [`super::ArtifactEdit`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditTarget {
    /// Registered artifact (`file:/…` URI on wire).
    Location(ArtifactLoc),
    /// Absolute project path — primary authoring form; implicit slot overlay create.
    Path(LpPathBuf),
}
