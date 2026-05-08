//! New runtime spine contracts (tick/destroy/memory pressure, narrow contexts).
//! Legacy runtimes live in [`crate::nodes`].

mod contexts;
pub mod node_entry;
pub mod node_entry_state;
mod node_error;
mod node_runtime;
pub mod node_tree;
pub mod sync;
pub mod tree_error;

pub use crate::memory::pressure_level::PressureLevel;
pub use contexts::{DestroyCtx, MemPressureCtx, NodeResourceInitContext, TickContext};
pub use node_entry::NodeEntry;
pub use node_entry_state::NodeEntryState;
pub use node_error::NodeError;
pub use node_runtime::NodeRuntime;
pub use node_tree::NodeTree;
pub use sync::tree_deltas_since;
pub use tree_error::TreeError;

#[cfg(test)]
pub(crate) fn test_placeholder_spine() -> (lpc_model::NodeInvocation, crate::artifact::ArtifactId) {
    (
        lpc_model::NodeInvocation::new(lpc_model::ArtifactLocator::path("__test__.vis")),
        crate::artifact::ArtifactId::from_raw(0),
    )
}
