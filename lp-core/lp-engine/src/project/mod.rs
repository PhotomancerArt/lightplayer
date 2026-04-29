pub mod loader;
pub mod project_runtime;

pub use loader::{discover_nodes, load_from_filesystem, load_node};
pub use project_runtime::{MemoryStatsFn, NodeEntry, NodeStatus, ProjectRuntime};

// Re-export API types for convenience
pub use lpc_model::project::api::{ApiNodeSpecifier, ProjectRequest};
pub use lpl_model::{NodeChange, NodeDetail, NodeState, ProjectResponse};
