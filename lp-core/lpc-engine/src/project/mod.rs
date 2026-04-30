pub mod hooks;
pub mod loader;
pub mod project_runtime;

pub use hooks::ProjectHooks;
pub use loader::{discover_nodes, load_from_filesystem, load_node};
pub use project_runtime::{MemoryStatsFn, NodeEntry, NodeStatus, ProjectRuntime};

// Re-export API types for convenience
pub use lpc_wire::{ApiNodeSpecifier, WireProjectRequest};
pub use lpl_model::{NodeChange, NodeDetail, NodeState, ProjectResponse};
