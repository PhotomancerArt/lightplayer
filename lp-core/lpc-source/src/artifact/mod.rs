//! Re-exports for **artifact**-related schema traits and loading.

pub mod load_artifact;
pub mod src_artifact;

pub use crate::schema::{Migration, Registry};
pub use lpc_model::artifact::artifact_loc::ArtifactLocator;
pub use load_artifact::{load_artifact, ArtifactReadRoot, LoadError};
pub use src_artifact::SrcArtifact;
pub use lpc_model::artifact::src_artifact_lib_ref::SrcArtifactLibRef;
