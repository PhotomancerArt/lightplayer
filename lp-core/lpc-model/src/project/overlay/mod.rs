//! Pending project edit overlay.
//!
//! A [`ProjectOverlay`] holds uncommitted user-authored changes to project
//! artifacts. The registry applies an overlay over committed artifact bodies to
//! derive an effective [`crate::ProjectInventory`].
//!
//! Overlay data is edit intent, not materialized artifact bytes for every
//! artifact. Slot overlays describe structured edits to node-definition TOML;
//! asset overlays replace or delete whole artifact bodies.
//!
//! Related modules:
//!
//! - [`crate::project::overlay_mutation`] defines command-shaped mutations that
//!   update overlays.
//! - [`crate::project::inventory`] defines the effective read model produced
//!   after overlay application.

pub mod artifact_overlay;
pub mod asset_body_overlay;
pub mod project_overlay;
pub mod slot_edit;
pub mod slot_overlay;

pub use crate::project::overlay_mutation::{
    MutationCmd, MutationCmdBatch, MutationCmdBatchResult, MutationCmdId, MutationCmdResult,
    MutationCmdStatus, MutationEffect, MutationOp, MutationRejection, MutationRejectionReason,
};
pub use artifact_overlay::ArtifactOverlay;
pub use asset_body_overlay::AssetBodyOverlay;
pub use project_overlay::ProjectOverlay;
pub use slot_edit::{SlotEdit, SlotEditOp};
pub use slot_overlay::SlotOverlay;
