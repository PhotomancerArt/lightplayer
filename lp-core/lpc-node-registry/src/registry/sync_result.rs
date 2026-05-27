//! Summary returned by [`super::NodeDefRegistry::sync`].

use alloc::vec::Vec;

use lpc_model::NodeKind;

use super::{NodeDefLoc, NodeDefUpdates};

/// Factual classification of a def change (not engine policy).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefChangeDetail {
    Content,
    KindChanged { from: NodeKind, to: NodeKind },
    EnteredError,
    LeftError,
}

/// Factual diff after applying a change batch and updating registry state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncResult {
    pub def_updates: NodeDefUpdates,
    pub change_details: Vec<(NodeDefLoc, DefChangeDetail)>,
}

impl SyncResult {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty() && self.change_details.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.def_updates.merge(other.def_updates);
        self.change_details.extend(other.change_details);
    }
}
