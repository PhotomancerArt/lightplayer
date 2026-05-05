//! LightPlayer **authored source** model: on-disk artifacts, slots, bindings, and value specs.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod legacy;
pub mod node;
pub mod presentation;
pub mod prop;
pub mod schema;

pub use artifact::{
    ArtifactLocator, ArtifactReadRoot, LoadError, Migration, Registry, SrcArtifact,
    SrcArtifactLibRef, load_artifact,
};
pub use node::{NodeDef, NodeInvocation, ProjectDef};
pub use presentation::Presentation;
pub use prop::{
    BindingResolver, FromTomlError, LoadCtx, SrcBinding, SrcShape, SrcSlot, SrcTextureSpec,
    SrcValueSpec, kind_default_bind, kind_default_presentation,
};
