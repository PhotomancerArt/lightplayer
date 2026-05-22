//! Node definition registry and artifact freshness store (parallel stack for M6 cutover).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod registry;
pub mod source;
pub mod view;

mod change;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState, ArtifactStore,
};
pub use registry::{
    DefSource, NodeDefEntry, NodeDefId, NodeDefRegistry, NodeDefState, NodeDefUpdates, ParseCtx,
    RegistryError, ValidationErrorPlaceholder,
};
pub use source::{
    MaterializeError, MaterializedSource, ResolveError, SourceDiagnosticCtx, SourceFileRef,
    materialize_source, resolve_source_file,
};
pub use view::NodeDefView;
