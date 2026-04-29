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
pub mod binding;
pub mod constraint;
pub mod error;
pub mod kind;
pub mod node;
pub mod presentation;
pub mod schema;
pub mod shape;
pub mod types;
pub mod value_spec;

// --- Protocol / project surface --------------------------------------------------------------

pub mod config;
pub mod json;
pub mod message;
pub mod nodes;
pub mod path;
pub mod project;
pub mod serde_base64;
pub mod serial;
pub mod server;
pub mod state;
pub mod transport_error;

// --- Foundation re-exports ------------------------------------------------------------------

/// Cross-cutting error for [`NodeProperties`](node::NodeProperties) property access and related domain operations.
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
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
pub use kind::Kind;
pub use node::NodeProperties;
pub use presentation::Presentation;
pub use schema::{Artifact, Migration, Registry};
pub use shape::{Shape, Slot};
pub use types::{
    ArtifactSpec, ChannelName, Name, NodePath, NodePathSegment, NodePropSpec, PathError, PropPath,
    Uid,
};
pub use value_spec::ValueSpec;

pub use artifact::{ArtifactReadRoot, LoadError, load_artifact};

// --- Protocol re-exports --------------------------------------------------------------------

pub use config::LightplayerConfig;
pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use nodes::{NodeHandle, NodeSpecifier};
pub use path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use project::{FrameId, ProjectConfig};
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use server::{AvailableProject, FsRequest, FsResponse, LoadedProject};
pub use transport_error::TransportError;
