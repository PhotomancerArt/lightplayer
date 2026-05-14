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
    write_project_read_response, write_project_read_result_json, write_project_read_server_message,
    write_server_message,
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
    JsonSyntaxSource, SlotJsonArray, SlotJsonObject, SlotJsonValue, SlotJsonWriter, SlotReader,
    SlotTomlError, SyntaxError, SyntaxEvent, SyntaxEventSource, SyntaxNode, TomlSyntaxSource,
    WireSlotChange, WireSlotFullSync, WireSlotMutationId, WireSlotMutationOp,
    WireSlotMutationRejection, WireSlotMutationRequest, WireSlotMutationResponse,
    WireSlotMutationResult, WireSlotPatch, WireSlotRootSnapshot, WireSlotRootsSnapshot,
    build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff, decode_slot_data_toml,
    decode_slot_data_toml_with_ignored_fields, encode_slot_data_access_toml, encode_slot_data_toml,
    snapshot_slot_root, snapshot_slot_shape, write_slot_data_json,
    write_slot_shape_registry_snapshot_json,
};
pub use transport_error::TransportError;
pub use tree::{WireChildKind, WireEntryState, WireSlotIndex, WireTreeDelta};

/// Canonical project-read message envelope.
pub type WireMessage = Message<ProjectReadResponse>;
/// Canonical project-read server message.
pub type WireServerMessage = ServerMessage<ProjectReadResponse>;
/// Canonical project-read server message body.
pub type WireServerMsgBody = ServerMsgBody<ProjectReadResponse>;
