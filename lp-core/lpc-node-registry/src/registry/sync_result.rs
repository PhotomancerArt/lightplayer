//! Summary returned by [`super::NodeDefRegistry::sync`].

use alloc::vec::Vec;

use lpc_model::{NodeKind, Revision};

use super::{NodeDefId, NodeDefUpdates};

/// One def whose resolved source version increased without a def TOML change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRevisionBump {
    pub def_id: NodeDefId,
    pub before: Revision,
    pub after: Revision,
}

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
    pub source_revisions: Vec<SourceRevisionBump>,
    pub change_details: Vec<(NodeDefId, DefChangeDetail)>,
}

impl SyncResult {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty()
            && self.source_revisions.is_empty()
            && self.change_details.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.def_updates.merge(other.def_updates);
        self.source_revisions.extend(other.source_revisions);
        self.change_details.extend(other.change_details);
    }
}
