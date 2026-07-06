//! Project node controller-domain types.
//!
//! A project node has two identities in Studio. [`ProjectNodeAddress`] is the
//! stable authored address used to preserve controller state across syncs.
//! [`ProjectNodeTarget`] adds the current runtime `NodeId` for actions that
//! need to talk back to the server.

pub mod node_controller;
pub mod node_revert_op;
pub mod project_node_address;
pub mod project_node_target;

pub(in crate::app::project) use node_controller::root_slot_key;
pub use node_controller::{NodeController, NodeControllerState, ProjectProductSubscriptionIntent};
pub use node_revert_op::NodeRevertOp;
pub use project_node_address::ProjectNodeAddress;
pub use project_node_target::ProjectNodeTarget;
