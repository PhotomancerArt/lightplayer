//! Artifact addressing for pending and committed files.

use lpfs::LpPathBuf;

use crate::ArtifactId;

/// Target file for an [`super::ArtifactEdit`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditTarget {
    /// Committed artifact handle.
    Id(ArtifactId),
    /// Absolute project path — primary authoring form; implicit slot overlay create.
    Path(LpPathBuf),
}
