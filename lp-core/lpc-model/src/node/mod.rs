//! Shared node identities, authored definition locators, and project use locators.

pub mod kind;
pub mod node_def_location;
pub mod node_id;
pub mod node_name;
/// Legacy node/property string parser from the pre-slot property model.
///
/// New code should prefer [`crate::SlotRef`] for slot endpoints and
/// [`crate::ValueRef`] only when it explicitly needs to project inside an
/// atomic slot value.
pub mod node_prop_spec;
pub mod node_use_location;
pub mod relative_node_ref;
pub mod tree_path;

pub use crate::nodes::node_def::{NodeArtifact, NodeDef};
pub use crate::project::inventory::node_def_entry::NodeDefEntry;
pub use crate::project::inventory::node_def_state::{NodeDefState, NodeDefValidationError};
pub use crate::project::overlay_mutation::node_def_change_summary::{
    NodeDefChange, NodeDefChangeKind, NodeDefChangeSummary,
};
pub use crate::slots::node_invocation_slot::{NodeInvocation, NodeInvocationSlot};
pub use kind::NodeKind;
pub use node_def_location::NodeDefLocation;
pub use node_id::NodeId;
pub use node_name::{NodeName, NodeNameError};
pub use node_use_location::{LocationSeg, NodeUseLocation};
pub use relative_node_ref::{RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc};
pub use tree_path::TreePath;
