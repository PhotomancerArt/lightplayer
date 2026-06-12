//! New runtime spine contracts (tick/destroy/memory pressure, narrow contexts).
//! Legacy runtimes live in [`crate::nodes`].

pub mod catch_node_panic;
mod contexts;
mod control_node;
mod node_binding_index;
mod node_call;
mod node_def_handle;
pub mod node_entry;
pub mod node_entry_state;
mod node_error;
mod node_runtime;
pub mod node_tree;
mod render_node;
mod runtime_state_shape;
pub mod sync;
pub mod tree_error;

pub use crate::engine::memory_pressure::PressureLevel;
pub use contexts::{
    ControlRenderContext, ControlRenderServices, DestroyCtx, MemPressureCtx,
    NodeResourceInitContext, RenderContext, TickContext, VisualRenderServices,
};
pub use control_node::ControlNode;
pub use node_call::{NodeCall, NodeCallKey};
pub use node_def_handle::NodeDefHandle;
pub use node_entry::RuntimeNodeEntry;
pub use node_entry_state::NodeEntryState;
pub use node_error::NodeError;
pub use node_runtime::{NodeRuntime, ProduceResult};
pub use node_tree::RuntimeNodeTree;
pub use render_node::RenderNode;
pub use runtime_state_shape::RuntimeStateShape;
pub use sync::tree_deltas_since;
pub use tree_error::TreeError;

#[cfg(test)]
pub(crate) fn test_placeholder_spine() -> (lpc_model::NodeInvocation, crate::artifact::ArtifactId) {
    (
        lpc_model::NodeInvocation::new(lpc_model::ArtifactSpec::path("__test__.vis")),
        crate::artifact::ArtifactId::from_raw(0),
    )
}
