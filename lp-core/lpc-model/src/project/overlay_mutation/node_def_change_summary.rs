//! Effective node definition inventory changes.
//!
//! These changes are derived by comparing two effective definition inventories.
//! They tell consumers which definition identities entered, left, or changed
//! state.

use crate::{ChangeSummary, NodeDefLocation, NodeKind};

/// Effective node definition changes visible to runtime/project consumers.
pub type NodeDefChangeSummary = ChangeSummary<NodeDefLocation, NodeDefChange>;

/// One changed node definition and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeDefChange {
    /// Changed definition identity.
    pub location: NodeDefLocation,
    /// Coarse classification of the change.
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
    /// Definition content changed without changing node kind.
    Body,
    /// Definition changed from one node kind to another.
    KindChanged { from: NodeKind, to: NodeKind },
    /// Definition moved from loaded state to an error state.
    EnteredError,
    /// Definition moved from an error state to loaded state.
    LeftError,
}
