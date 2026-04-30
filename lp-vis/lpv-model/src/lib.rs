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
    artifact, constraint, error, kind, node, presentation, schema, types, value_spec,
};

// Re-export foundation types from lpc-model for convenience
pub use lpc_model::{
    Binding, Constraint, ConstraintChoice, ConstraintFree, ConstraintRange, DomainError, Kind,
    LpsType, LpsValue, NodeId, NodeName, Presentation, Shape, Slot, TextureBuffer,
    TextureStorageFormat, ValueSpec,
};

pub use lpc_model::artifact::{LoadError, load_artifact};
// Re-export foundation types from lpc-model for convenience
pub use lpc_model::ArtifactSpec;
pub use lpc_model::ChannelName;
// Re-export foundation types from lpc-model for convenience
pub use lpc_model::node::node_props::NodeProps;
// Re-export foundation types from lpc-model for convenience
pub use lpc_model::tree::tree_path::TreePath;
// Re-export foundation types from lpc-model for convenience
pub use lpc_model::node::node_prop_spec::NodePropSpec;
// Foundation modules (same paths as upstream `lp-domain` / `lpv-model` for `crate::kind::` etc.)
pub use lpc_model::prop::binding;
// Re-export foundation types from lpc-model for convenience
pub use lpc_model::prop::prop_path::PropPath;
// Foundation modules (same paths as upstream `lp-domain` / `lpv-model` for `crate::kind::` etc.)
pub use lpc_model::prop::shape;
