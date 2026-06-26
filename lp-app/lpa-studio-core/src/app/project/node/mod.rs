//! Project node controller-domain types.
//!
//! A project node has two identities in Studio. [`ProjectNodeAddress`] is the
//! stable authored address used to preserve controller state across syncs.
//! [`ProjectNodeTarget`] adds the current runtime `NodeId` for actions that
//! need to talk back to the server.

pub mod project_node_address;
pub mod project_node_controller;
pub mod project_node_descriptor;
pub mod project_node_target;

pub use project_node_address::ProjectNodeAddress;
pub use project_node_controller::{
    ProjectNodeController, ProjectNodeControllerState, ProjectProductSubscriptionIntent,
};
pub use project_node_descriptor::ProjectNodeDescriptor;
pub use project_node_target::ProjectNodeTarget;
