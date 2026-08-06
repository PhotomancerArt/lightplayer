//! One tip of a project's history DAG.

use alloc::vec::Vec;
use lpc_cloud_api::HeadInfo;
use lpc_history::ContentHash;

/// A single head: the tree hash of a tip commit, and the parents it was
/// pushed with.
///
/// The parents are recorded because they are what a later push is matched
/// against when the frontier is updated — and because a client pulling a
/// second head needs to know where it branched from to build the join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRef {
    /// Tree hash of this head's commit.
    pub tree: ContentHash,
    /// Parent tree hashes the commit was pushed with.
    pub parents: Vec<ContentHash>,
}

impl HeadRef {
    /// The client-facing view of this head.
    pub fn to_head_info(&self) -> HeadInfo {
        HeadInfo {
            tree: self.tree,
            parents: self.parents.clone(),
        }
    }
}
