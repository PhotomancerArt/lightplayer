//! LightPlayer **authored source** model: on-disk artifacts and artifact loading.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
mod artifact_read_root;
pub mod legacy;
pub mod presentation;
pub mod prop;
pub mod schema;

pub mod slot_shapes {
    pub use lpc_model::slot_shapes::*;
}

pub use artifact::{
    ArtifactLocator, ArtifactReadRoot, LoadError, Migration, Registry, SrcArtifact,
    SrcArtifactLibRef, load_artifact,
};
pub use presentation::Presentation;
pub use prop::{SrcBinding, SrcShape, SrcSlot, SrcTextureSpec, SrcValueSpec};
