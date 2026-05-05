//! **Shared** graph node identifiers and authored node-tree locators.

pub mod node_id;
pub mod node_loc;
pub mod node_name;
pub mod node_prop_spec;

pub use crate::tree::tree_path::TreePath;
pub use node_id::NodeId;
pub use node_loc::NodeLoc;
pub use node_name::{NodeName, NodeNameError};
