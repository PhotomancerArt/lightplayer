//! LightPlayer data models and protocol definitions.
//!
//! This crate defines the core data structures and message types used throughout
//! the LightPlayer system, including:
//! - Project and node configurations
//! - Client-server message protocol
//! - File system API definitions
//! - Path handling utilities

#![no_std]

extern crate alloc;

pub mod config;
pub mod glsl_opts;
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

pub use config::LightplayerConfig;
pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use nodes::{NodeConfig, NodeHandle, NodeKind, NodeSpecifier};
pub use path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use project::{FrameId, ProjectConfig};
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use server::{AvailableProject, FsRequest, FsResponse, LoadedProject};
pub use transport_error::TransportError;

// Legacy type aliases for the protocol envelope. The `R` type parameter on
// `Message` / `ServerMessage` / `ServerMsgBody` is pinned to
// `SerializableProjectResponse` here for the existing legacy stack
// (Texture / Shader / Output / Fixture). These aliases will move out of
// lp-model when the crate is split into lpc-model + lpl-model
// (see docs/roadmaps/2026-04-28-node-runtime/m2-crate-restructure/).
pub type LegacyMessage = message::Message<project::api::SerializableProjectResponse>;
pub type LegacyServerMessage = message::ServerMessage<project::api::SerializableProjectResponse>;
pub type LegacyServerMsgBody =
    server::api::ServerMsgBody<project::api::SerializableProjectResponse>;
