//! Effective project node definition inventory entry.

use crate::{NodeDefLocation, NodeDefState, Revision};

/// One referenced node definition in the effective project inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    pub location: NodeDefLocation,
    pub state: NodeDefState,
    pub revision: Revision,
}

impl NodeDefEntry {
    pub fn new(location: NodeDefLocation, state: NodeDefState, revision: Revision) -> Self {
        Self {
            location,
            state,
            revision,
        }
    }
}
