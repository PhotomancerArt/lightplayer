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
pub use sync::{tree_deltas_since, tree_deltas_since_iter};
pub use tree_error::TreeError;

/// Size a node's persistent scratch buffer, failing softly on OOM.
///
/// Per-tick read-back sinks hold one reusable buffer and resize it here
/// instead of materializing a fresh `Vec` every tick (the per-tick clone
/// was enough to reset a playing classic — see the flash-write-wedge
/// defect's memory findings). Growth goes through `try_reserve_exact`, so
/// on a memory-starved device the tick fails with a [`NodeError`] instead
/// of aborting inside the allocator.
pub fn ensure_scratch_len<T: Clone + Default>(
    scratch: &mut alloc::vec::Vec<T>,
    len: usize,
) -> Result<(), NodeError> {
    let additional = len.saturating_sub(scratch.len());
    if additional > 0 {
        scratch.try_reserve_exact(additional).map_err(|_| {
            NodeError::msg(alloc::format!(
                "scratch buffer allocation failed ({len} elements)"
            ))
        })?;
    }
    scratch.resize(len, T::default());
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_placeholder_spine() -> lpc_model::NodeInvocation {
    lpc_model::NodeInvocation::new(lpc_model::ArtifactSpec::path("__test__.vis"))
}
