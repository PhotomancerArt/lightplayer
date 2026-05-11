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
pub mod slot;
pub mod state;
pub mod transport_error;
pub mod tree;

pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use project::{
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, VisualProductPayloadOptions,
    VisualProductPayloadRequest, VisualProductPayloadSpecifier, WireChannelSampleFormat,
    WireColorLayout, WireNodeSlotRoot, WireNodeStatus, WireProjectHandle, WireProjectRequest,
    WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
    WireResourceSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
    WireRuntimeBufferPayload, WireSlotRootKind, WireSlotWatchSpecifier, WireTextureFormat,
    WireVisualProductKind, WireVisualProductPayload,
};
pub use server::{
    AvailableProject, ClientMsgBody, FsRequest, FsResponse, LoadedProject, MemoryStats,
    SampleStats, ServerConfig, ServerMsgBody,
};
pub use slot::{
    WireSlotChange, WireSlotFullSync, WireSlotMutationId, WireSlotMutationOp,
    WireSlotMutationRejection, WireSlotMutationRequest, WireSlotMutationResponse,
    WireSlotMutationResult, WireSlotPatch, WireSlotRootSnapshot, build_slot_full_sync,
    collect_slot_diff, snapshot_slot_root, snapshot_slot_shape,
};
pub use transport_error::TransportError;
pub use tree::{WireChildKind, WireEntryState, WireSlotIndex, WireTreeDelta};

/// Temporary project-response-free message envelope used between M2.2 demolition and M3 sync.
pub type WireMessage = Message<NoDomain>;
/// Temporary project-response-free server message used between M2.2 demolition and M3 sync.
pub type WireServerMessage = ServerMessage<NoDomain>;
/// Temporary project-response-free server message body used between M2.2 demolition and M3 sync.
pub type WireServerMsgBody = ServerMsgBody<NoDomain>;
