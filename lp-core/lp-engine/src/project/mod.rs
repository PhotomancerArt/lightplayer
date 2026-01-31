pub mod loader;
pub mod runtime;

pub use loader::{discover_nodes, load_from_filesystem, load_node};
pub use runtime::{NodeEntry, NodeStatus, ProjectRuntime};

// Re-export API types for convenience
pub use lp_model::project::api::{
    ApiNodeSpecifier, NodeChange, NodeDetail, NodeState, ProjectRequest, ProjectResponse,
};
