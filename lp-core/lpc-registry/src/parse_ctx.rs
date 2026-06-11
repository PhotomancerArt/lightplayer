//! Registry parsing context.

use lpc_model::SlotShapeRegistry;

/// Shared model parsing context for authored node definitions.
#[derive(Clone, Copy)]
pub struct ParseCtx<'a> {
    pub shapes: &'a SlotShapeRegistry,
}
