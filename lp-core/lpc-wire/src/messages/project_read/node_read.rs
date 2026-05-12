//! Node-centric project read query/result.

use super::ReadLevel;
use crate::slot::WireSlotRootsSnapshot;
use crate::tree::WireTreeDelta;
use alloc::vec::Vec;
use lpc_model::NodeId;

/// Which nodes should be included in a node read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum NodeReadSelection {
    All,
    ByIds(Vec<NodeId>),
}

impl Default for NodeReadSelection {
    fn default() -> Self {
        Self::All
    }
}

/// Request node tree and node-associated slot detail.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeReadQuery {
    pub level: ReadLevel,
    #[serde(default)]
    pub nodes: NodeReadSelection,
    #[serde(default)]
    pub include_slots: bool,
}

impl NodeReadQuery {
    #[must_use]
    pub fn detail_all() -> Self {
        Self {
            level: ReadLevel::Detail,
            nodes: NodeReadSelection::All,
            include_slots: true,
        }
    }
}

/// Node read result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeReadResult {
    pub level: ReadLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree_deltas: Vec<WireTreeDelta>,
    pub slots: Option<WireSlotRootsSnapshot>,
}
