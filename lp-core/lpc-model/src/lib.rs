//! LightPlayer **core model** crate: wire protocol definitions and **foundation**
//! types (identity, addressing, Quantity model, artifact schema traits).
//!
//! Legacy node configs (Texture / Shader / Output / Fixture) live in `lpl-model`.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// --- Foundation (Quantity model + artifact traits) -----------------------------------------

pub mod artifact;
pub use prop::binding;
pub use prop::constraint;
pub mod error;
pub use prop::kind;
pub mod node;
pub mod presentation;
pub mod prop;
pub mod schema;
pub mod types;
pub mod value_spec;

// --- Protocol / project surface --------------------------------------------------------------

pub mod bus;
pub mod json;
pub mod lp_config;
pub mod lp_path;
pub mod message;
pub mod project;
pub mod serde_base64;
pub mod serial;
pub mod server;
pub mod state;
pub mod transport_error;
// --- Foundation re-exports ------------------------------------------------------------------

/// Cross-cutting error for [`NodeProps`](node::node_props::NodeProps) property access and related domain operations.
pub use error::DomainError;
/// Shader-facing structural type system (mirrors [`LpsValue`]); shared with the GLSL/compilation stack.
pub use lps_shared::LpsType;
/// Canonical structural **value** type for the engine and tooling.
pub use lps_shared::LpsValueF32 as LpsValue;
/// Opaque texture pixel storage (lives beside handle values in the GPU/loader story).
pub use lps_shared::TextureBuffer;
/// Texture format id for [`Kind::Texture`](kind::Kind::Texture) storage.
pub use lps_shared::TextureStorageFormat;

pub use binding::Binding;
pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
pub use kind::Kind;
pub use node::node_props::NodeProps;
pub use presentation::Presentation;
pub use prop::Prop;
pub use prop::shape::{Shape, Slot};
pub use value_spec::ValueSpec;

pub use artifact::{Artifact, ArtifactReadRoot, ArtifactSpec, LoadError, load_artifact};

// --- Protocol re-exports --------------------------------------------------------------------

pub use lp_config::LightplayerConfig;
pub use lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use node::node_path::{NodePath, NodePathSegment, PathError};
pub use node::node_prop_spec::NodePropSpec;
pub use node::{NodeId, NodeName, NodeNameError, NodeSpec};
/// Legacy name for [`NodeSpec`] (`lpc_model::nodes::NodeSpecifier` in older call sites).
pub type NodeSpecifier = NodeSpec;

/// Legacy module path `lpc_model::nodes::*`; prefer [`node`] and crate-root [`NodeId`] / [`NodeSpec`].
pub mod nodes {
    pub use super::{NodeId, NodeSpecifier};
}

pub use project::{FrameId, ProjectConfig};
pub use prop::prop_path::PropPath;
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use server::{AvailableProject, FsRequest, FsResponse, LoadedProject};
pub use transport_error::TransportError;
