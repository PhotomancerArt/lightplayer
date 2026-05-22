//! Def-level delta report returned by [`super::NodeDefRegistry::sync`].

use alloc::vec::Vec;

use super::NodeDefId;

/// Added, changed, and removed node definitions after a registry sync.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeDefUpdates {
    pub added: Vec<NodeDefId>,
    pub changed: Vec<NodeDefId>,
    pub removed: Vec<NodeDefId>,
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

    pub fn push_added(&mut self, id: NodeDefId) {
        push_unique(&mut self.added, id);
    }

    pub fn push_changed(&mut self, id: NodeDefId) {
        push_unique(&mut self.changed, id);
    }

    pub fn push_removed(&mut self, id: NodeDefId) {
        push_unique(&mut self.removed, id);
    }

    pub fn contains_changed(&self, id: NodeDefId) -> bool {
        self.changed.contains(&id)
    }
}

fn push_unique(list: &mut Vec<NodeDefId>, id: NodeDefId) {
    if !list.contains(&id) {
        list.push(id);
    }
}
