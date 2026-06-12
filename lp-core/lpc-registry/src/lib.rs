//! Effective project registry built from artifacts plus a pending overlay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod asset;
pub(crate) mod overlay;

pub mod registry;
#[cfg(any(test, feature = "diff"))]
pub mod test;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactLocation, ArtifactReadFailure, ArtifactReadState,
    ArtifactStore,
};
pub use asset::{MaterializeAssetError, MaterializedAsset, MaterializedTextAsset};
pub use lpc_model::{
    ArtifactOverlay, AssetOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use overlay::{EditApplyError, serialize_slot_draft};
pub use registry::commit_error::CommitError;
pub use registry::load_result::LoadResult;
pub use registry::parse_ctx::ParseCtx;
pub use registry::project_registry::ProjectRegistry;
pub use registry::registry_error::RegistryError;
#[cfg(feature = "diff")]
pub use test::snapshot_overlay::{
    ProjectSnapshot, SnapshotError, derive_overlay_between_snapshots,
};
