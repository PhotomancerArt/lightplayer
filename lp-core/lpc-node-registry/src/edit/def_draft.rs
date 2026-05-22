//! Mutable node-def draft for slot overlay edits.

use lpc_model::NodeDef;

/// Pending slot tree for one `.toml` artifact path.
#[derive(Clone, Debug, PartialEq)]
pub struct DefDraft {
    pub def: NodeDef,
}

impl DefDraft {
    pub fn new(def: NodeDef) -> Self {
        Self { def }
    }
}
