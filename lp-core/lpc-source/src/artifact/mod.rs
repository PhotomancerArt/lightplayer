//! Re-exports for **artifact**-related schema traits and loading.

pub mod artifact_loc;
pub mod load_artifact;
pub mod src_artifact;
pub mod src_artifact_lib_ref;

pub use crate::schema::{Migration, Registry};
pub use artifact_loc::ArtifactLocator;
pub use load_artifact::{ArtifactReadRoot, LoadError, load_artifact};
pub use src_artifact::SrcArtifact;
pub use src_artifact_lib_ref::SrcArtifactLibRef;
