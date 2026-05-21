//! Read-only view over base registry defs (ChangeSet overlay in M5).

use crate::registry::{NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState};

/// Base registry lookup; M5 adds ChangeSet projection.
pub struct NodeDefView<'a> {
    registry: &'a NodeDefRegistry,
}

impl<'a> NodeDefView<'a> {
    pub fn new(registry: &'a NodeDefRegistry) -> Self {
        Self { registry }
    }

    pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry> {
        self.registry.get(id)
    }

    pub fn state(&self, id: &NodeDefId) -> Option<&NodeDefState> {
        self.registry.get(id).map(|entry| &entry.state)
    }
}
