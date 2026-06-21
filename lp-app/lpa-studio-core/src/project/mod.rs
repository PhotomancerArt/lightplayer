//! Project manager model for attaching Studio to a project on a ready server.
//!
//! This layer owns project selection, project loading/deploying, project
//! inventory snapshots, and future sync/edit state. It does not own link/device
//! operations or raw server transport.

pub mod project_action;
pub mod project_choice;
pub mod project_selection_reason;
pub mod project_state;
pub mod project_state_result;

pub use project_action::ProjectActionRequest;
pub use project_state::{ProjectState, ProjectSyncState};
