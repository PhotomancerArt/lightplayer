//! Project-level model types.
//!
//! This module owns the shared vocabulary for loaded LightPlayer projects:
//! referenced node definitions, referenced assets, the effective project node
//! tree, pending overlays, and the mutation/change-set shapes used to edit
//! overlays. It deliberately stays below `lpc-registry`: these are portable
//! model types, while registry code performs filesystem reads, parsing, overlay
//! application, and materialization.
//!
//! Related modules:
//!
//! - [`crate::nodes`] defines authored [`crate::NodeDef`] payloads and
//!   [`crate::NodeInvocation`] slot values.
//! - [`crate::artifact`] defines artifact identities used by project assets and
//!   node definitions.
//! - [`crate::slot`] defines [`crate::SlotPath`], the path language used by
//!   overlays and project node locations.

pub mod asset;
pub mod config;
pub mod inventory;
pub mod overlay;
pub mod overlay_mutation;
pub mod overlay_commit;

pub use crate::sync::current_revision::{advance_revision, current_revision, set_current_revision};
pub use crate::sync::revision::Revision;
pub use overlay_commit::commit_result::CommitResult;
pub use config::ProjectConfig;
pub use inventory::project_inventory::ProjectInventory;
pub use inventory::project_node::{ProjectNode, ProjectNodeOrigin};
pub use inventory::project_node_location::{LocationSeg, ProjectNodeLocation};
pub use inventory::project_node_placement::ProjectNodePlacement;
pub use inventory::project_tree::ProjectTree;
pub use overlay_mutation::mutation_result::{MutationBatchResults, MutationResult};
pub use overlay_mutation::project_change_set::ProjectChangeSet;
