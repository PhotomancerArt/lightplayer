//! One parsed node definition at a [`super::NodeDefLoc`].

use lpc_model::Revision;

use super::{NodeDefLoc, NodeDefState};

/// Parsed or failed node definition at a stable source address.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    pub loc: NodeDefLoc,
    pub state: NodeDefState,
    pub revision: Revision,
}
