//! New runtime spine contracts (tick/destroy/memory pressure, narrow contexts).
//! Legacy runtimes live in [`crate::nodes`].

pub mod catch_node_panic;
mod contexts;
mod control_node;
mod node_binding_index;
mod node_call;
pub mod node_entry;
pub mod node_entry_state;
mod node_error;
mod node_runtime;
pub mod node_tree;
mod render_node;
mod runtime_state_shape;
pub mod scope;
// Every current caller is a feature-gated node runtime, so the helper is
// gated on their union to keep minimal builds warning-clean.
#[cfg(any(
    feature = "node-fixture",
    feature = "node-playlist",
    feature = "node-shader"
))]
mod scratch;
pub mod sync;
pub mod tree_error;

pub use crate::engine::memory_pressure::PressureLevel;
pub use contexts::{
    AssetRefreshContext, ControlRenderContext, ControlRenderServices, DestroyCtx, MemPressureCtx,
    NodeResourceInitContext, RenderContext, TickContext, TimebaseRead, VisualRenderServices,
};
pub use control_node::ControlNode;
pub use node_call::{NodeCall, NodeCallKey};
pub use node_entry::RuntimeNodeEntry;
pub use node_entry_state::NodeEntryState;
pub use node_error::NodeError;
pub(crate) use node_error::err_ctx;
pub use node_runtime::{AssetRefreshResult, NodeRuntime, PatchedRun, ProduceResult};
pub use node_tree::RuntimeNodeTree;
pub use render_node::RenderNode;
pub use runtime_state_shape::RuntimeStateShape;
pub use scope::ScopeRef;
#[cfg(any(
    feature = "node-fixture",
    feature = "node-playlist",
    feature = "node-shader"
))]
pub(crate) use scratch::ensure_scratch_len;
pub use sync::{tree_deltas_since, tree_deltas_since_iter};
pub use tree_error::TreeError;

#[cfg(test)]
pub(crate) fn test_placeholder_spine() -> lpc_model::NodeInvocation {
    lpc_model::NodeInvocation::new(lpc_model::ArtifactSpec::path("__test__.vis"))
}
