//! Node specifier for sync/API requests over the wire (`WireNodeSpecifier`).

use alloc::vec::Vec;
use lpc_model::node::NodeId;
use serde::{Deserialize, Serialize};

/// Node specifier for sync/API requests over the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireNodeSpecifier {
    /// No nodes.
    None,
    /// All nodes.
    All,
    /// Specific handles.
    ByHandles(Vec<NodeId>),
}
