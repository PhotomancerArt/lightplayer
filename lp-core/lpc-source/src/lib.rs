//! LightPlayer **authored source** model: on-disk artifacts and artifact loading.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod legacy;
pub mod node;
pub mod presentation;
pub mod schema;
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}

pub use artifact::{
    ArtifactLocator, ArtifactReadRoot, LoadError, Migration, Registry, SrcArtifact,
    SrcArtifactLibRef, load_artifact,
};
pub use node::{NodeDef, NodeInvocation, ProjectDef};
pub use presentation::Presentation;
