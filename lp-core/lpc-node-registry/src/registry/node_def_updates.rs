//! Def-level delta report returned by [`super::NodeDefRegistry::sync`].

use alloc::collections::BTreeSet;

use super::NodeDefId;

/// Added, changed, and removed node definitions after a registry sync.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeDefUpdates {
    pub added: BTreeSet<NodeDefId>,
    pub changed: BTreeSet<NodeDefId>,
    pub removed: BTreeSet<NodeDefId>,
}

impl NodeDefUpdates {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.added.extend(other.added);
        self.changed.extend(other.changed);
        self.removed.extend(other.removed);
    }
}
