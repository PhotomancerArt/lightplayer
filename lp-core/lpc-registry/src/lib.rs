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
pub use project::commit_error::CommitError;
pub use edit::{serialize_slot_draft, EditApplyError};
pub use project::load_result::LoadResult;
pub use lpc_model::{
    AssetOverlay, ArtifactOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use project::parse_ctx::ParseCtx;
pub use project::project_registry::ProjectRegistry;
pub use project::registry_error::RegistryError;
#[cfg(feature = "diff")]
pub use project::snapshot_overlay::{derive_overlay_between_snapshots, ProjectSnapshot, SnapshotError};
pub use source::{
    materialize_source, resolve_source_file, MaterializeError, MaterializedSource, ResolveError,
    SourceDiagnosticCtx, SourceFileRef,
};
