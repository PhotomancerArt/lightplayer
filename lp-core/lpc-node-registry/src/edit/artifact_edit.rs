//! One artifact block in an [`super::EditBatch`].

use alloc::vec::Vec;

use super::{EditOp, EditTarget};

/// Edits targeting a single artifact path or id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactEdit {
    pub target: EditTarget,
    pub ops: Vec<EditOp>,
}
