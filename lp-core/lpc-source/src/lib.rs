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
    Artifact, ArtifactReadRoot, ArtifactSpec, LoadError, Migration, Registry, load_artifact,
};
pub use node::{NodeConfig, SrcNodeConfig};
pub use presentation::Presentation;
pub use prop::{
    BindingResolver, FromTomlError, LoadCtx, SrcBinding, SrcShape, SrcSlot, SrcTextureSpec,
    SrcValueSpec, kind_default_bind, kind_default_presentation,
};
// `Src*` names are primary; keep short-term aliases expected by current call sites.
pub type Binding = SrcBinding;
pub type Shape = SrcShape;
pub type Slot = SrcSlot;
pub type TextureSpec = SrcTextureSpec;
pub type ValueSpec = SrcValueSpec;
