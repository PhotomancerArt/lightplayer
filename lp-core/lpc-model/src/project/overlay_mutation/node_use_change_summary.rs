//! Effective node-use inventory changes.
//!
//! These changes are derived by comparing two effective project trees. They
//! describe topology-facing changes to node uses, not authored value changes
//! inside the referenced definitions.

use crate::{ChangeSummary, NodeDefLocation, NodeUseLocation};

/// Effective node-use changes visible to runtime/project consumers.
pub type NodeUseChangeSummary = ChangeSummary<NodeUseLocation, NodeUseChange>;

/// One changed node use and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeUseChange {
    /// Changed node-use identity.
    pub location: NodeUseLocation,
    /// Coarse classification of the change.
    pub kind: NodeUseChangeKind,
}

impl NodeUseChange {
    pub fn new(location: NodeUseLocation, kind: NodeUseChangeKind) -> Self {
        Self { location, kind }
    }
}

/// Runtime-facing node-use change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeUseChangeKind {
    /// The use now points at a different node definition.
    DefinitionChanged {
        /// Previously resolved definition.
        from: NodeDefLocation,
        /// Currently resolved definition.
        to: NodeDefLocation,
    },
    /// The use moved under a different parent.
    ParentChanged,
    /// The authored slot or placement that produced this use changed.
    OriginChanged,
}
