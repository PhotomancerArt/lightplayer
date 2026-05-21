//! Parse context for reading authored node TOML.

use lpc_model::SlotShapeRegistry;

/// Shape registry passed into TOML parse during registry load and sync.
pub struct ParseCtx<'a> {
    pub shapes: &'a SlotShapeRegistry,
}
