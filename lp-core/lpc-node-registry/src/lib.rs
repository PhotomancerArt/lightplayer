//! Node definition registry with artifact freshness and client edit overlay.
//!
//! [`ArtifactStore`] owns the project file catalog ([`ArtifactLocation`] URIs,
//! freshness, transient reads). [`NodeDefRegistry`] is a consumer: parsed
//! def entries plus a [`ProjectOverlay`] for uncommitted pending edits.
//! [`NodeDefView`] exposes effective reads (overlay ∪ committed). Mutate pending
//! state with [`NodeDefRegistry::upsert_slot_edit`] / [`NodeDefRegistry::set_pending_artifact_body`],
//! then [`NodeDefRegistry::commit`] or [`NodeDefRegistry::discard_overlay`].
//!
//! With the `diff` feature (default on host, omit on embedded), [`diff`] returns an
//! [`ProjectOverlay`] between project snapshots for harness and replay.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
#[cfg(feature = "diff")]
pub mod diff;
pub mod edit;
pub(crate) mod edit_apply;
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
pub use edit::{CommitError, EditError};
pub use lpc_model::{
    ArtifactBodyEdit, ArtifactOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
#[allow(deprecated, reason = "legacy sync op alias for migration")]
pub use registry::RegistryChange;
pub use registry::{
    NodeDefChangeDetail, NodeDefEntry, NodeDefLocation, NodeDefRegistry, NodeDefState,
    NodeDefUpdates, NodeDefValidationError, ParseCtx, RegistryError, SyncError, SyncOp,
    SyncOutcome, SyncResult, serialize_slot_draft,
};
pub use source::{
    MaterializeError, MaterializedSource, ResolveError, SourceDiagnosticCtx, SourceFileRef,
    materialize_source, resolve_source_file,
};
pub use view::NodeDefView;
