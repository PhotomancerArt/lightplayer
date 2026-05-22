//! One artifact block in a [`super::ChangeSet`].

use alloc::vec::Vec;

use super::{ArtifactOp, ArtifactTarget};

/// Ops targeting a single artifact path or id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactChange {
    pub target: ArtifactTarget,
    pub ops: Vec<ArtifactOp>,
}
