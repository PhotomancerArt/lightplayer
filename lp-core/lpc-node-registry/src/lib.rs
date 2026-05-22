//! Node definition registry and artifact freshness store (parallel stack for M6 cutover).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod change;
pub mod registry;
pub mod source;
pub mod view;

#[cfg(test)]
pub mod harness;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState, ArtifactStore,
};
pub use change::{
    ArtifactChange, ArtifactOp, ArtifactTarget, ChangeError, ChangeOverlay, ChangeSet, ChangeSetId,
    CommitError, OverlayEntry, SlotDraft,
};
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
