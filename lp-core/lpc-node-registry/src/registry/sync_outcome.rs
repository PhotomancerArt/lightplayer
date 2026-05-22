//! Combined pending and committed effects from [`super::NodeDefRegistry::sync`].

use super::SyncResult;

/// Result of processing a [`super::SyncOp`] batch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncOutcome {
    /// Committed registry effects (filesystem + commit legs).
    pub committed: SyncResult,
    /// Whether any op in the batch mutated the pending overlay.
    pub pending_changed: bool,
}
