//! Effective project registry built from artifacts plus a pending overlay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub(crate) mod edit;
pub mod source;

#[cfg(test)]
pub mod harness;
pub mod project;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactLocation, ArtifactReadFailure, ArtifactReadState,
    ArtifactStore,
};
pub use edit::{EditApplyError, serialize_slot_draft};
pub use lpc_model::{
    ArtifactOverlay, AssetOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use project::commit_error::CommitError;
pub use project::load_result::LoadResult;
pub use project::parse_ctx::ParseCtx;
pub use project::project_registry::ProjectRegistry;
pub use project::registry_error::RegistryError;
#[cfg(feature = "diff")]
pub use project::snapshot_overlay::{
    ProjectSnapshot, SnapshotError, derive_overlay_between_snapshots,
};
pub use source::{
    MaterializeError, MaterializedSource, ResolveError, SourceDiagnosticCtx, SourceFileRef,
    materialize_source, resolve_source_file,
};
