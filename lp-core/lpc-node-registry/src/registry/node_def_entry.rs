//! One registry entry keyed by [`super::NodeDefId`].

use lpc_model::Revision;

use super::{DefSource, NodeDefId, NodeDefState};

/// Parsed or failed node definition at a stable source address.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    pub id: NodeDefId,
    pub source: DefSource,
    pub state: NodeDefState,
    pub last_seen_revision: Revision,
}
