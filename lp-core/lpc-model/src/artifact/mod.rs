//! Re-exports for **artifact**-related schema traits. Concrete struct types
//! (`Pattern`, `Effect`, `Transition`, `Stack`, `Live`, `Playlist` — the six
//! **Visual** / playlist kinds in `docs/roadmaps/2026-04-22-lp-domain/overview.md`)
//! and TOML loaders are **M3+**; M2 only wires [`Artifact`], [`Migration`],
//! and [`Registry`] from [`crate::schema`].

pub mod load;

pub use crate::schema::{Artifact, Migration, Registry};

pub use load::{ArtifactReadRoot, LoadError, load_artifact};
