//! **Shared** graph node identifiers and authored node-tree locators.

pub mod node_id;
pub mod node_name;
/// Legacy node/property string parser from the pre-slot property model.
///
/// New code should prefer [`crate::SlotRef`] for slot endpoints and
/// [`crate::ValueRef`] only when it explicitly needs to project inside an
/// atomic slot value.
pub mod node_prop_spec;
pub mod relative_node_ref;
pub mod kind;
pub mod node_def;
pub mod node_invocation;

pub use crate::tree::tree_path::TreePath;
pub use kind::NodeKind;
pub use node_def::NodeDef;
pub use node_id::NodeId;
pub use node_invocation::NodeInvocation;
pub use node_name::{NodeName, NodeNameError};
pub use relative_node_ref::{RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc};
