//! Effective project registry built from artifacts plus a pending overlay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub(crate) mod overlay;
pub mod source;

pub mod registry;
#[cfg(any(test, feature = "diff"))]
pub mod test;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactLocation, ArtifactReadFailure, ArtifactReadState,
    ArtifactStore,
};
pub use lpc_model::{
    ArtifactOverlay, AssetOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use overlay::{EditApplyError, serialize_slot_draft};
pub use registry::commit_error::CommitError;
pub use registry::load_result::LoadResult;
pub use registry::parse_ctx::ParseCtx;
pub use registry::project_registry::ProjectRegistry;
pub use registry::registry_error::RegistryError;
pub use source::materialize::MaterializedSource;
pub use source::{
    MaterializeError, ResolveError, SourceDiagnosticCtx, SourceFileRef, materialize_source,
    resolve_source_file,
};
#[cfg(feature = "diff")]
pub use test::snapshot_overlay::{
    ProjectSnapshot, SnapshotError, derive_overlay_between_snapshots,
};
