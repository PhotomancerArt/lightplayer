//! Mutable node-def draft for overlay slot edits.

use lpc_model::NodeDef;

/// Pending slot tree for one `.toml` artifact path.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotDraft {
    pub def: NodeDef,
}

impl SlotDraft {
    pub fn new(def: NodeDef) -> Self {
        Self { def }
    }
}
