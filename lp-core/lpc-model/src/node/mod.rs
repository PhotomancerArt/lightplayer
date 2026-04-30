//! **Shared** graph node identifiers and specs (`NodeId`, `NodeSpec`, names,
//! paths).

pub mod node_id;
pub mod node_name;
pub mod node_prop_spec;
pub mod node_spec;

pub use crate::tree::tree_path::TreePath;
pub use node_id::NodeId;
pub use node_name::{NodeName, NodeNameError};
pub use node_spec::NodeSpec;
