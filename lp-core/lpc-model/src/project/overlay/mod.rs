//! Project overlay model
//!
//! ProjectOverlay holds uncommitted user-authored changes to a project.
//!
//! These changes are reflected in the engine and its runtime nodes.
//!
//! Once a user is satisfied with the effects of their changes, the overlay can be committed.
//!
//!

pub mod artifact_overlay;
pub mod asset_overlay;
pub mod project_commit_summary;
pub mod project_overlay;
pub mod slot_edit;
pub mod slot_overlay;

pub use artifact_overlay::ArtifactOverlay;
pub use asset_overlay::AssetOverlay;
pub use project_commit_summary::ProjectCommitSummary;
pub use project_overlay::ProjectOverlay;
pub use slot_edit::{SlotEdit, SlotEditOp};
pub use slot_overlay::SlotOverlay;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmd;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmdId;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmdResult;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmdStatus;
pub use crate::project::overlay_mutation::mutation_cmd::MutationEffect;
pub use crate::project::overlay_mutation::mutation_cmd::MutationRejectionReason;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmdBatch;
pub use crate::project::overlay_mutation::mutation_cmd::MutationCmdBatchResult;
pub use crate::project::overlay_mutation::mutation_op::MutationOp;
pub use crate::project::overlay_mutation::mutation_cmd::MutationRejection;
