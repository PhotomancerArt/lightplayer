//! Artifact ergonomics: re-exports of the trait shapes from `crate::schema`.
//!
//! Concrete artifact types (`Pattern`, `Stack`, `Live`, `Playlist`, `Setlist`,
//! `Show`) land in M3 alongside their TOML loaders.

pub use crate::schema::{Artifact, Migration, Registry};
