//! LightPlayer **authored source** model: on-disk artifacts, slots, bindings, and value specs.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod node;
pub mod presentation;
pub mod prop;
pub mod schema;

pub use artifact::{
    ArtifactReadRoot, LoadError, Migration, Registry, SrcArtifact, SrcArtifactLibRef,
    SrcArtifactSpec, load_artifact,
};
pub use node::SrcNodeConfig;
pub use presentation::Presentation;
pub use prop::{
    BindingResolver, FromTomlError, LoadCtx, SrcBinding, SrcShape, SrcSlot, SrcTextureSpec,
    SrcValueSpec, kind_default_bind, kind_default_presentation,
};
