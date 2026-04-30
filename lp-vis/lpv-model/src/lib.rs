//! lpv-model: visual types (Pattern, Effect, Stack, Playlist, etc.).
//! Foundation types live in [`lpc_model`]; authored source types in [`lpc_source`].

#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod visual;

#[cfg(feature = "schema-gen")]
mod schema_gen_smoke;

pub use visual::{
    Effect, EffectRef, Live, LiveCandidate, ParamsTable, Pattern, Playlist, PlaylistBehavior,
    PlaylistEntry, ShaderRef, Stack, Transition, TransitionRef, VisualInput,
};

pub use lpc_model::{constraint, error, kind, node, types};
pub use lpc_source::{artifact, presentation, schema};

pub use lpc_model::tree::tree_path::TreePath;
pub use lpc_model::{
    ChannelName, Constraint, ConstraintChoice, ConstraintFree, ConstraintRange, DomainError, Kind,
    NodeId, NodeName, NodePropSpec, PropPath,
};
pub use lpc_source::prop::{binding, shape};
pub use lpc_source::{
    LoadError, Presentation, SrcArtifactSpec, SrcBinding, SrcNodeConfig, SrcShape, SrcSlot,
    SrcTextureSpec, SrcValueSpec, load_artifact,
};
pub use lps_shared::{LpsType, LpsValueF32, TextureBuffer, TextureStorageFormat};
