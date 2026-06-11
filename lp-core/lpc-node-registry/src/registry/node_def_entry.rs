//! One parsed node definition at a [`super::NodeDefLocation`].

use lpc_model::Revision;

use super::{NodeDefLocation, NodeDefState};

/// Parsed or failed node definition at a stable definition address.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    pub loc: NodeDefLocation,
    pub state: NodeDefState,
    pub revision: Revision,
}
