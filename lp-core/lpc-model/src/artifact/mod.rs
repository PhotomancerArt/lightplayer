//! Re-exports for **artifact**-related schema traits. Concrete struct types
//! (`Pattern`, `Effect`, `Transition`, `Stack`, `Live`, `Playlist` — the six
//! **Visual** / playlist kinds in `docs/roadmaps/2026-04-22-lp-domain/overview.md`)
//! and TOML loaders are **M3+**; M2 only wires [`Artifact`], [`Migration`],
//! and [`Registry`] from [`crate::schema`].

pub mod artifact;
pub mod artifact_spec;
pub mod load_artifact;

pub use crate::schema::{Migration, Registry};

pub use artifact::Artifact;
pub use artifact_spec::ArtifactSpec;
pub use load_artifact::{ArtifactReadRoot, LoadError, load_artifact};
