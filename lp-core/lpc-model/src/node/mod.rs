//! **Shared** graph node identifiers and authored node-tree locators.

pub mod node_id;
pub mod node_name;
pub mod node_prop_spec;
pub mod relative_node_ref;

pub use crate::tree::tree_path::TreePath;
pub use node_id::NodeId;
pub use node_name::{NodeName, NodeNameError};
pub use relative_node_ref::{RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc};
