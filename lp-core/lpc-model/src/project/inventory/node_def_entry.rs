use crate::{NodeDefLocation, NodeDefState, Revision};

/// One referenced node definition in the effective project inventory.
///
/// The registry stores one entry for each referenced [`crate::NodeDefLocation`].
/// Entries can be loaded successfully or preserve an error state so clients can
/// display missing/invalid project structure without losing identity.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDefEntry {
    /// Definition identity.
    pub location: NodeDefLocation,
    /// Loaded definition or structured failure state.
    pub state: NodeDefState,
    /// Effective revision of this definition state.
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
