//! LightPlayer engine↔client wire model (`Wire*` types where needed).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod budget;
pub mod json;
pub mod message;
pub mod messages;
pub mod project;
pub mod project_command;
pub mod project_inventory;
pub mod project_overlay;
#[cfg(feature = "ser-write-json")]
pub mod ser_write;
pub mod serde_base64;
pub mod server;
pub mod slot;
pub mod transport_error;
pub mod tree;

pub use messages::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlDisplayLayoutProbeResult,
    ControlDisplayLayoutRead, ControlProductProbeRequest, ControlProductProbeResult,
    ControlProductProbeResultHeader, NodeReadQuery, NodeReadResult, NodeReadSelection,
    PROJECT_READ_FRAME_MAX_BYTES, PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES,
    PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES, PROJECT_READ_RUNTIME_CHUNK_BYTES, ProjectProbeRequest,
    ProjectProbeResult, ProjectProbeResultHeader, ProjectReadEvent, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    ProjectReadResourceEvent, ProjectReadShapeEvent, ProjectRuntimeStatus, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, RenderProductProbeResultHeader,
    ResourcePayloadRead, ResourceReadQuery, ResourceReadResult, RuntimeReadQuery,
    RuntimeReadResult, ServerRuntimeStatus, ShapeReadQuery, ShapeReadResult, WireBindingDirection,
    WireBindingEndpoint, WireBindingGraph, WireBindingOrigin, WireBusChannel, WireBusChannelValue,
    WireEffectiveBinding,
};
pub use messages::{ClientMessage, ClientRequest, Message, ServerMessage};
pub use project::{
    NodeRuntimeStatus, WireChannelSampleFormat, WireColorLayout, WireProjectHandle,
    WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
    WireResourceSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
    WireRuntimeBufferPayload, WireTextureFormat,
};
pub use project_command::{WireProjectCommand, WireProjectCommandResponse};
pub use project_inventory::{WireProjectInventoryReadRequest, WireProjectInventoryReadResponse};
pub use project_overlay::{
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
};
#[cfg(feature = "ser-write-json")]
pub use ser_write::{CountingSerWrite, ser_write_json_len};
pub use server::{
    AvailableProject, ClientMsgBody, FsRequest, FsResponse, FwProvenance, LoadedProject,
    MemoryStats, SampleStats, ServerConfig, ServerHello, ServerMsgBody, WIRE_PROTO_VERSION,
};
pub use slot::{
    WireSlotChange, WireSlotData, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot,
    WireSlotRootsSnapshot, build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff,
    snapshot_slot_root, snapshot_slot_shape, wire_slot_data_from_slot_access,
};
pub use transport_error::TransportError;
pub use tree::{WireChildKind, WireEntryState, WireSlotIndex, WireTreeDelta};

/// Canonical project-read message envelope.
pub type WireMessage = Message;
/// Canonical project-read server message.
pub type WireServerMessage = ServerMessage;
/// Canonical project-read server message body.
pub type WireServerMsgBody = ServerMsgBody;
