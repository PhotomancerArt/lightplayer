//! lpv-model: visual types (Pattern, Effect, Stack, Playlist, etc.).
//! Foundation types live in [`lpc_model`].

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

// Foundation modules (same paths as upstream `lp-domain` / `lpv-model` for `crate::kind::` etc.)
pub use lpc_model::{
    artifact, binding, constraint, error, kind, node, presentation, schema, shape, types,
    value_spec,
};

// Re-export foundation types from lpc-model for convenience
pub use lpc_model::{
    ArtifactSpec, Binding, ChannelName, Constraint, ConstraintChoice, ConstraintFree,
    ConstraintRange, DomainError, Kind, LpsType, LpsValue, Name, NodePath, NodePropSpec,
    NodeProperties, Presentation, PropPath, Shape, Slot, TextureBuffer, TextureStorageFormat, Uid,
    ValueSpec,
};

pub use lpc_model::artifact::{LoadError, load_artifact};
