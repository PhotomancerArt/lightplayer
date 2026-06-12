//! Effective project inventory.
//!
//! Inventory is the read-side model produced after project artifacts and the
//! current [`crate::ProjectOverlay`] have been combined. It contains:
//!
//! - unique referenced node definitions keyed by [`crate::NodeDefLocation`],
//! - unique referenced assets keyed by [`crate::AssetSource`],
//! - an expanded [`crate::ProjectTree`] of node occurrences.
//!
//! The registry owns deriving inventory; these types only describe the portable
//! model shape.

pub mod node_def_entry;
pub mod node_def_location;
pub mod node_def_state;
pub mod project_inventory;
pub mod project_node;
pub mod project_node_location;
pub mod project_node_placement;
pub mod project_tree;
