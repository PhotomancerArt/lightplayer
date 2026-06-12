//! Definition update summaries.
//!
//! These are compact summaries for consumers that need to know which
//! definitions were added, changed, or removed without carrying full before/after
//! inventory snapshots.

use alloc::vec::Vec;

use crate::{NodeDefLocation, NodeKind};

/// Added, changed, and removed node definitions.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefUpdates {
    /// Newly referenced definition locations.
    pub added: Vec<NodeDefLocation>,
    /// Previously referenced definition locations whose effective state changed.
    pub changed: Vec<NodeDefLocation>,
    /// Definition locations that are no longer referenced.
    pub removed: Vec<NodeDefLocation>,
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

    pub fn push_added(&mut self, loc: NodeDefLocation) {
        push_unique(&mut self.added, loc);
    }

    pub fn push_changed(&mut self, loc: NodeDefLocation) {
        push_unique(&mut self.changed, loc);
    }

    pub fn push_removed(&mut self, loc: NodeDefLocation) {
        push_unique(&mut self.removed, loc);
    }

    pub fn contains_changed(&self, loc: &NodeDefLocation) -> bool {
        self.changed.contains(loc)
    }
}

/// Factual classification of a definition change.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeDefChangeDetail {
    /// Definition body changed without changing node kind.
    Content,
    /// Definition body changed to another node kind.
    KindChanged { from: NodeKind, to: NodeKind },
    /// Definition moved from loaded state into an error state.
    EnteredError,
    /// Definition moved from an error state into loaded state.
    LeftError,
}

fn push_unique(list: &mut Vec<NodeDefLocation>, loc: NodeDefLocation) {
    if !list.contains(&loc) {
        list.push(loc);
    }
}
