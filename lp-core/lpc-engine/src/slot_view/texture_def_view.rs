//! Read-only resolver-backed view of [`lpc_model::TextureDef`].

use crate::node::{NodeError, TickContext};
use lpc_model::{Dim2u, SlotPath};

/// Typed helper for reading texture definition fields through the resolver.
pub struct TextureDefView<'a, 'ctx> {
    ctx: &'a mut TickContext<'ctx>,
}

impl<'a, 'ctx> TextureDefView<'a, 'ctx> {
    pub fn new(ctx: &'a mut TickContext<'ctx>) -> Self {
        Self { ctx }
    }

    pub fn size(&mut self) -> Result<Dim2u, NodeError> {
        self.ctx.resolve_consumed_slot_value(&size_path())
    }
}

fn size_path() -> SlotPath {
    SlotPath::parse("size").expect("texture size slot path is valid")
}
