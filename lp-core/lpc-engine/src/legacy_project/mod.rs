pub mod hooks;
pub mod legacy_loader;
pub mod project_runtime;

pub use hooks::LegacyProjectHooks;
pub use legacy_loader::{discover_nodes, legacy_load_from_filesystem, legacy_load_node};
pub use project_runtime::{MemoryStatsFn, NodeEntry, NodeStatus, LegacyProjectRuntime};

// Re-export API types for convenience
pub use lpc_wire::{WireNodeSpecifier, WireProjectRequest};
pub use lpl_model::{NodeChange, NodeDetail, NodeState, ProjectResponse};
