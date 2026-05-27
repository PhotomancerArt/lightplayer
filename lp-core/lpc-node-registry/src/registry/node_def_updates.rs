//! Def-level delta report returned by [`super::NodeDefRegistry::sync`].

use alloc::vec::Vec;

use super::NodeDefLoc;

/// Added, changed, and removed node definitions after a registry sync.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeDefUpdates {
    pub added: Vec<NodeDefLoc>,
    pub changed: Vec<NodeDefLoc>,
    pub removed: Vec<NodeDefLoc>,
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

    pub fn push_added(&mut self, loc: NodeDefLoc) {
        push_unique(&mut self.added, loc);
    }

    pub fn push_changed(&mut self, loc: NodeDefLoc) {
        push_unique(&mut self.changed, loc);
    }

    pub fn push_removed(&mut self, loc: NodeDefLoc) {
        push_unique(&mut self.removed, loc);
    }

    pub fn contains_changed(&self, loc: &NodeDefLoc) -> bool {
        self.changed.contains(loc)
    }
}

fn push_unique(list: &mut Vec<NodeDefLoc>, loc: NodeDefLoc) {
    if !list.contains(&loc) {
        list.push(loc);
    }
}
