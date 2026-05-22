//! Effective read projection over committed registry entries and overlay draft.

use lpfs::LpFs;

use crate::registry::{NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState, ParseCtx};

/// Effective def lookup — overlay ∪ committed cache.
pub struct NodeDefView<'a> {
    registry: &'a NodeDefRegistry,
}

impl<'a> NodeDefView<'a> {
    pub fn new(registry: &'a NodeDefRegistry) -> Self {
        Self { registry }
    }

    /// Effective def entry (overlay ∪ base). Always owned.
    pub fn get(&self, id: &NodeDefId, _fs: &dyn LpFs, ctx: &ParseCtx<'_>) -> Option<NodeDefEntry> {
        self.registry.effective_entry(id, ctx)
    }

    pub fn state(
        &self,
        id: &NodeDefId,
        _fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefState> {
        self.registry.effective_state(id, ctx)
    }
}
