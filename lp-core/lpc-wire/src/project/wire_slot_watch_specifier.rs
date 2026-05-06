//! Slot-root watch specifiers for generic project sync.

use alloc::vec::Vec;
use lpc_model::NodeId;
use serde::{Deserialize, Serialize};

/// Conventional top-level slot roots a node may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum WireSlotRootKind {
    /// Authored source/definition data.
    Source,
    /// Runtime state exposed for inspection.
    State,
    /// Runtime parameters, often materialized from authored definitions.
    Params,
    /// Runtime products or other primary node outputs.
    Output,
}

/// One node/root pair to include in generic slot sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireNodeSlotRoot {
    pub node: NodeId,
    pub root: WireSlotRootKind,
}

/// Client interest in generic node slot data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum WireSlotWatchSpecifier {
    /// Do not include generic slot data.
    #[default]
    None,
    /// Include all nodes' `state` roots.
    AllState,
    /// Include all conventional roots for all nodes.
    All,
    /// Include only the listed node/root pairs.
    ByRoots(Vec<WireNodeSlotRoot>),
}
