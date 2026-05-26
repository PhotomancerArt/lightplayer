//! One registry entry keyed by [`super::NodeDefId`].

use lpc_model::Revision;

use super::{NodeDefId, NodeDefLoc, NodeDefState};

/// Parsed or failed node definition at a stable source address.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    pub id: NodeDefId,
    pub loc: NodeDefLoc,
    pub state: NodeDefState,
    pub revision: Revision,
}
