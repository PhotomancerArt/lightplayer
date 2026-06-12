//! Command and result types for mutating a project overlay.
//!
//! Overlay mutations are message-shaped operations. They update
//! [`crate::ProjectOverlay`] and report resulting effective project changes,
//! but they do not themselves read files or apply edits to artifacts. The
//! registry owns that execution.
//!
//! Related modules:
//!
//! - [`crate::project::overlay`] stores canonical pending edit intent.
//! - [`crate::project::inventory`] stores the effective project state used to
//!   compute change summaries.

pub mod asset_change_summary;
mod mutation_cmd;
mod mutation_op;
pub mod mutation_result;
pub mod node_def_change_summary;
pub mod project_change_summary;

pub use mutation_cmd::{
    MutationCmd, MutationCmdBatch, MutationCmdBatchResult, MutationCmdId, MutationCmdResult,
    MutationCmdStatus, MutationEffect, MutationRejection, MutationRejectionReason,
};
pub use mutation_op::MutationOp;
