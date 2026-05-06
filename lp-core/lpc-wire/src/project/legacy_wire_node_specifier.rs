//! Legacy node-detail specifier for compatibility sync requests.

use alloc::vec::Vec;
use lpc_model::node::NodeId;
use serde::{Deserialize, Serialize};

/// Legacy selector for compatibility node config/state details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyWireNodeSpecifier {
    /// No nodes.
    None,
    /// All nodes.
    All,
    /// Specific handles.
    ByHandles(Vec<NodeId>),
}
