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

pub mod json;
pub mod message;
pub mod nodes;
pub mod path;
pub mod project;
pub mod serde_base64;
pub mod serial;
pub mod server;
pub mod transport_error;

pub use message::{ClientMessage, ClientRequest, Message, ServerMessage};
pub use nodes::{NodeConfig, NodeHandle, NodeKind, NodeSpecifier};
pub use path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use project::{FrameId, ProjectConfig};
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use server::{AvailableProject, FsRequest, FsResponse, LoadedProject};
pub use transport_error::TransportError;
