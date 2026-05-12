//! LightPlayer engine↔client wire model (`Wire*` types where needed).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod json;
pub mod message;
pub mod messages;
pub mod project;
pub mod serde_base64;
pub mod server;
pub mod slot;
pub mod state;
pub mod transport_error;
pub mod tree;

pub use messages::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use messages::{
    ExplainSlotProbeRequest, ExplainSlotProbeResult, NodeReadQuery, NodeReadResult,
    NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResponseWriter, ProjectReadResult,
    ReadLevel, RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead,
    ResourceReadQuery, ResourceReadResult, ShapeReadQuery, ShapeReadResult, SlotExplanation,
    write_project_read_response,
};
pub use project::{
    WireChannelSampleFormat, WireColorLayout, WireNodeStatus, WireProjectHandle,
    WireProjectRequest, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
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

/// Canonical project-read message envelope.
pub type WireMessage = Message<ProjectReadResponse>;
/// Canonical project-read server message.
pub type WireServerMessage = ServerMessage<ProjectReadResponse>;
/// Canonical project-read server message body.
pub type WireServerMsgBody = ServerMsgBody<ProjectReadResponse>;
