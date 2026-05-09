//! Read-only resolver-backed view of [`lpc_model::TextureDef`].

use crate::node::{NodeError, TickContext};
use lpc_model::{Dim2u, SlotAccessorError, SlotShapeRegistry, SlotViewRoot};

/// Typed helper for reading texture definition fields through the resolver.
pub struct TextureDefView {
    inner: lpc_model::TextureDefSlotView,
}

impl TextureDefView {
    /// Compile view accessors against the current shape registry revision.
    pub fn compile(registry: &SlotShapeRegistry) -> Result<Self, SlotAccessorError> {
        Ok(Self {
            inner: lpc_model::TextureDef::compile_slot_view(registry)?,
        })
    }

    pub fn registry_revision(&self) -> lpc_model::Revision {
        self.inner.registry_revision()
    }

    pub fn is_valid_for(&self, registry: &SlotShapeRegistry) -> bool {
        self.inner.is_valid_for(registry)
    }

    pub fn size(&self, ctx: &mut TickContext<'_>) -> Result<Dim2u, NodeError> {
        ctx.resolve_consumed_slot_accessor_value(self.inner.size())
    }
}
