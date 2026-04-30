//! Re-exports for **artifact**-related schema traits and loading.

pub mod load_artifact;
pub mod src_artifact;
pub mod src_artifact_spec;

pub use crate::schema::{Migration, Registry};
pub use load_artifact::{ArtifactReadRoot, LoadError, load_artifact};
pub use src_artifact::SrcArtifact;
pub use src_artifact_spec::SrcArtifactSpec;
