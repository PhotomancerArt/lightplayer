//! LightPlayer **core model** crate: **foundation** types (identity, addressing,
//! Quantity model). Wire/protocol shapes live in `lpc-wire`.
//!
//! Legacy node configs (Texture / Shader / Output / Fixture) live in `lpc_source::legacy`.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// --- Foundation (Quantity model) -------------------------------------------------------------

pub mod error;
pub mod node;
pub mod prop;
pub mod types;

// --- Shared surface (non-wire) ---------------------------------------------------------------

pub mod bus;
pub mod lp_config;
pub mod lp_path;
pub mod serial;
pub mod tree;

pub mod project;

// --- Foundation re-exports ------------------------------------------------------------------

pub use prop::constraint;
pub use prop::kind;

pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
/// Cross-cutting error for domain property access and validation.
pub use error::DomainError;
pub use kind::Kind;
pub use prop::PropNamespace;
pub use prop::PropValue;
pub use prop::{ModelStructMember, ModelType, ModelValue};

pub use lp_config::LightplayerConfig;
pub use lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use node::node_prop_spec::NodePropSpec;
pub use node::{NodeId, NodeName, NodeNameError, NodeSpec};
pub use project::{FrameId, ProjectConfig};
pub use prop::prop_path::PropPath;
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use tree::tree_path::{NodePathSegment, PathError, TreePath};
