//! Effective read projection over committed registry entries and overlay drafts.
//!
//! [`NodeDefView`] is the public read surface for node defs: overlay edits win
//! over the committed parse cache without mutating stored entries.

use lpfs::LpFs;

use crate::registry::{NodeDefEntry, NodeDefLoc, NodeDefRegistry, NodeDefState, ParseCtx};

/// Effective def lookup — overlay ∪ committed cache.
pub struct NodeDefView<'a> {
    registry: &'a NodeDefRegistry,
}

impl<'a> NodeDefView<'a> {
    pub fn new(registry: &'a NodeDefRegistry) -> Self {
        Self { registry }
    }

    /// Effective def entry (overlay ∪ base). Always owned.
    pub fn get(
        &self,
        loc: &NodeDefLoc,
        _fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefEntry> {
        self.registry.effective_entry(loc, ctx)
    }

    pub fn state(
        &self,
        loc: &NodeDefLoc,
        _fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDefState> {
        self.registry.effective_state(loc, ctx)
    }
}
