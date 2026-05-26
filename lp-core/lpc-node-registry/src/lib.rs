//! Node definition registry with artifact freshness and client edit overlay.
//!
//! [`ArtifactStore`] owns the project file catalog ([`ArtifactLocation`] URIs,
//! freshness, transient reads). [`NodeDefRegistry`] is a consumer: parsed
//! def entries plus a [`SlotOverlay`] for uncommitted client edits.
//! [`NodeDefView`] exposes effective reads (slot overlay ∪ committed). Apply an
//! [`EditBatch`] with [`NodeDefRegistry::apply_edit_batch`], then [`NodeDefRegistry::commit`] or
//! [`NodeDefRegistry::discard_slot_overlay`].
//!
//! With the `diff` feature (default on host, omit on embedded), [`diff`] builds
//! an [`EditBatch`] between project snapshots for harness and replay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
#[cfg(feature = "diff")]
pub mod diff;
pub mod edit;
pub mod registry;
pub mod source;
pub mod view;

#[cfg(test)]
pub mod harness;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactLocation, ArtifactReadFailure, ArtifactReadState,
    ArtifactStore,
};
#[cfg(feature = "diff")]
pub use diff::{DiffError, ProjectSnapshot, assert_equivalent, diff};
pub use edit::{
    ArtifactEdit, AssetEdit, CommitError, DefDraft, EditBatch, EditBatchId, EditError, EditTarget,
    SlotEdit, SlotOverlay, SlotOverlayEntry,
};
#[allow(deprecated, reason = "legacy sync op alias for migration")]
pub use registry::RegistryChange;
pub use registry::{
    DefChangeDetail, NodeDefEntry, NodeDefId, NodeDefLoc, NodeDefRegistry, NodeDefState,
    NodeDefUpdates, ParseCtx, RegistryError, SyncError, SyncOp, SyncOutcome, SyncResult,
    ValidationErrorPlaceholder, serialize_slot_draft,
};
pub use source::{
    MaterializeError, MaterializedSource, ResolveError, SourceDiagnosticCtx, SourceFileRef,
    materialize_source, resolve_source_file,
};
pub use view::NodeDefView;

#[allow(deprecated, reason = "legacy edit type aliases for migration")]
mod legacy_edit_names {
    pub use super::edit::{
        ArtifactChange, ArtifactOp, ArtifactTarget, ChangeError, ChangeOverlay, ChangeSet,
        ChangeSetId, OverlayEntry, SlotDraft,
    };
}
#[deprecated(note = "renamed to edit module")]
pub use edit as change;
#[allow(deprecated, reason = "legacy edit type aliases for migration")]
pub use legacy_edit_names::{
    ArtifactChange, ArtifactOp, ArtifactTarget, ChangeError, ChangeOverlay, ChangeSet, ChangeSetId,
    OverlayEntry, SlotDraft,
};
