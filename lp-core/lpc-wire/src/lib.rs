//! LightPlayer engine↔client wire model (`Wire*` types where needed).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod json;
pub mod message;
pub mod messages;
pub mod project;
pub mod project_edit;
pub mod serde_base64;
pub mod server;
pub mod slot;
pub mod transport_error;
pub mod tree;

pub use messages::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use messages::{
    ExplainSlotProbeRequest, ExplainSlotProbeResult, NodeReadQuery, NodeReadResult,
    NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResponseWriter, ProjectReadResult,
    ProjectRuntimeStatus, ReadLevel, RenderProductProbeRequest, RenderProductProbeResult,
    ResourcePayloadRead, ResourceReadQuery, ResourceReadResult, RuntimeReadQuery,
    RuntimeReadResult, ServerRuntimeStatus, ShapeReadQuery, ShapeReadResult, SlotExplanation,
    write_project_read_response, write_project_read_result_json, write_project_read_server_message,
    write_server_message,
};
pub use project::{
    WireChannelSampleFormat, WireColorLayout, WireNodeStatus, WireProjectHandle,
    WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
    WireResourceSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
    WireRuntimeBufferPayload, WireTextureFormat,
};
pub use project_edit::{WireProjectEditRequest, WireProjectEditResponse};
pub use server::{
    AvailableProject, ClientMsgBody, FsRequest, FsResponse, LoadedProject, MemoryStats,
    SampleStats, ServerConfig, ServerMsgBody,
};
pub use slot::{
    WireSlotChange, WireSlotData, WireSlotFullSync, WireSlotMutationId, WireSlotMutationOp,
    WireSlotMutationRejection, WireSlotMutationRequest, WireSlotMutationResponse,
    WireSlotMutationResult, WireSlotPatch, WireSlotRootSnapshot, WireSlotRootsSnapshot,
    build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff, snapshot_slot_root,
    snapshot_slot_shape, wire_slot_data_from_slot_access, write_slot_shape_registry_snapshot_json,
};
pub use transport_error::TransportError;
pub use tree::{WireChildKind, WireEntryState, WireSlotIndex, WireTreeDelta};

/// Canonical project-read message envelope.
pub type WireMessage = Message<ProjectReadResponse>;
/// Canonical project-read server message.
pub type WireServerMessage = ServerMessage<ProjectReadResponse>;
/// Canonical project-read server message body.
pub type WireServerMsgBody = ServerMsgBody<ProjectReadResponse>;
