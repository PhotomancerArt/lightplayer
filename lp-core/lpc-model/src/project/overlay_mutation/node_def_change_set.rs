//! Effective node definition inventory changes.

use alloc::vec::Vec;

use crate::{NodeDefLocation, NodeKind};

/// Effective node definition changes visible to runtime/project consumers.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefChangeSet {
    pub added: Vec<NodeDefLocation>,
    pub changed: Vec<NodeDefChange>,
    pub removed: Vec<NodeDefLocation>,
}

impl NodeDefChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// One changed node definition and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefChange {
    pub location: NodeDefLocation,
    pub kind: NodeDefChangeKind,
}

impl NodeDefChange {
    pub fn new(location: NodeDefLocation, kind: NodeDefChangeKind) -> Self {
        Self { location, kind }
    }
}

/// Runtime-facing node definition change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeDefChangeKind {
    Body,
    KindChanged { from: NodeKind, to: NodeKind },
    EnteredError,
    LeftError,
}
