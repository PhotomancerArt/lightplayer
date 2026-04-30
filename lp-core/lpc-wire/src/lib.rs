//! LightPlayer engine↔client wire model (`Wire*` types where needed).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod json;
pub mod message;
pub mod project;
pub mod serde_base64;
pub mod server;
pub mod state;
pub mod transport_error;
pub mod tree;

pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use project::{ApiNodeSpecifier, WireNodeStatus, WireProjectHandle, WireProjectRequest};
pub use server::{
    AvailableProject, ClientMsgBody, FsRequest, FsResponse, LoadedProject, MemoryStats,
    SampleStats, ServerConfig, ServerMsgBody,
};
pub use transport_error::TransportError;
pub use tree::{SlotIdx, WireChildKind, WireEntryState, WireTreeDelta};
