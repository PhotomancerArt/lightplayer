//! Node definition registry with artifact freshness and client edit overlay.
//!
//! [`ArtifactStore`] tracks file freshness and transient reads without caching
//! bytes. [`NodeDefRegistry`] owns committed parse entries plus a
//! [`SlotOverlay`] for uncommitted client edits. [`NodeDefView`] exposes
//! effective reads (slot overlay ∪ committed). Apply an [`EditBatch`] with
//! [`NodeDefRegistry::apply_edit_batch`], then [`NodeDefRegistry::commit`] or
//! [`NodeDefRegistry::discard_slot_overlay`].
//!
//! With the `diff` feature (default on host, omit on embedded), [`diff`] builds
//! an [`EditBatch`] between project snapshots for harness and replay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod edit;
#[cfg(feature = "diff")]
pub mod diff;
pub mod registry;
pub mod source;
pub mod view;

#[cfg(test)]
pub mod harness;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState, ArtifactStore,
};
pub use edit::{
    ArtifactEdit, CommitError, DefDraft, EditBatch, EditBatchId, EditError, EditOp, EditTarget,
    SlotOverlay, SlotOverlayEntry,
};
#[cfg(feature = "diff")]
pub use diff::{DiffError, ProjectSnapshot, assert_equivalent, diff};
pub use registry::{
    DefChangeDetail, DefSource, NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState,
    NodeDefUpdates, ParseCtx, RegistryChange, RegistryError, SourceRevisionBump, SyncResult,
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
