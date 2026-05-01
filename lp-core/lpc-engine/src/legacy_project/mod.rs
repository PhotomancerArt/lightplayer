pub mod legacy_loader;
pub mod project_runtime;

pub use legacy_loader::{discover_nodes, legacy_load_from_filesystem, legacy_load_node};
pub use project_runtime::{LegacyProjectRuntime, MemoryStatsFn, NodeEntry, NodeStatus};

// Re-export API types for convenience
pub use lpc_wire::legacy::{NodeChange, NodeDetail, NodeState, ProjectResponse};
pub use lpc_wire::{WireNodeSpecifier, WireProjectRequest};
