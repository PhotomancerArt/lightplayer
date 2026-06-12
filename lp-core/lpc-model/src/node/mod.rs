//! **Shared** graph node identifiers and authored node-tree locators.

pub mod kind;
pub mod node_id;
pub mod node_name;
/// Legacy node/property string parser from the pre-slot property model.
///
/// New code should prefer [`crate::SlotRef`] for slot endpoints and
/// [`crate::ValueRef`] only when it explicitly needs to project inside an
/// atomic slot value.
pub mod node_prop_spec;
pub mod relative_node_ref;
pub mod tree_path;

pub use crate::nodes::node_def::{NodeArtifact, NodeDef};
pub use kind::NodeKind;
pub use crate::project::mutation::node_def_change_set::{NodeDefChange, NodeDefChangeKind, NodeDefChangeSet};
pub use crate::project::inventory::node_def_entry::NodeDefEntry;
pub use crate::project::inventory::node_def_location::NodeDefLocation;
pub use crate::project::inventory::node_def_state::{NodeDefState, NodeDefValidationError};
pub use crate::project::inventory::node_def_updates::{NodeDefChangeDetail, NodeDefUpdates};
pub use node_id::NodeId;
pub use crate::slots::node_invocation_slot::{NodeInvocation, NodeInvocationSlot};
pub use node_name::{NodeName, NodeNameError};
pub use relative_node_ref::{RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc};
pub use tree_path::TreePath;
