//! lpv-model: visual types (Pattern, Effect, Stack, Playlist, etc.).
//!
//! This disabled reference crate predates the slot-native `lpc-model` authored
//! project model. The old `lpc_source` crate has been retired; remaining source
//! files are archival sketches rather than an active compile target.

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

pub use lpc_model::{constraint, kind, node};
pub use lpc_model::node::tree_path::TreePath;
pub use lpc_model::{
    ChannelName, Constraint, ConstraintChoice, ConstraintFree, ConstraintRange, Kind, NodeId,
    NodeInvocation, NodeName, NodePropSpec, ValuePath,
};
pub use lps_shared::{LpsType, LpsValueF32, TextureBuffer, TextureStorageFormat};
