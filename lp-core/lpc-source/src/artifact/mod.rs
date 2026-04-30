//! Re-exports for **artifact**-related schema traits and loading.

pub mod artifact;
pub mod artifact_spec;
pub mod load_artifact;

pub use crate::schema::{Migration, Registry};
pub use artifact::Artifact;
pub use artifact_spec::ArtifactSpec;
pub use load_artifact::{ArtifactReadRoot, LoadError, load_artifact};
