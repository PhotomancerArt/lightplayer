//! LightPlayer **authored source** model: on-disk artifacts and artifact loading.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod legacy;
pub mod presentation;
pub mod schema;
pub mod slot_shapes {
    pub use lpc_model::slot_shapes::*;
}

pub use artifact::{
    load_artifact, ArtifactLocator, ArtifactReadRoot, LoadError, Migration, Registry,
    SrcArtifact, SrcArtifactLibRef,
};
pub use presentation::Presentation;
