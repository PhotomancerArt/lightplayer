//! Effective project registry built from artifacts plus a pending overlay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod apply_error;
pub mod artifact;
pub mod commit_error;
pub(crate) mod edit_apply;
mod inventory_change_set;
pub mod load_result;
pub mod parse_ctx;
mod project_inventory_derivation;
pub mod project_registry;
pub mod registry_error;
#[cfg(feature = "diff")]
pub mod snapshot_overlay;
pub mod source;

#[cfg(test)]
pub mod harness;

pub use apply_error::ApplyError;
pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactLocation, ArtifactReadFailure, ArtifactReadState,
    ArtifactStore,
};
pub use commit_error::CommitError;
pub use edit_apply::{EditError, serialize_slot_draft};
pub use load_result::LoadResult;
pub use lpc_model::{
    ArtifactBodyEdit, ArtifactOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use parse_ctx::ParseCtx;
pub use project_registry::ProjectRegistry;
pub use registry_error::RegistryError;
#[cfg(feature = "diff")]
pub use snapshot_overlay::{ProjectSnapshot, SnapshotError, derive_overlay_between_snapshots};
pub use source::{
    MaterializeError, MaterializedSource, ResolveError, SourceDiagnosticCtx, SourceFileRef,
    materialize_source, resolve_source_file,
};
