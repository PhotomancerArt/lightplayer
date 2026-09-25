//! LightPlayer engine↔client wire model (`Wire*` types where needed).
//!
//! A board writes its replies as JSON text (`M!{json}` lines) until a host
//! opts a link into **JSON Pack**, a compact binary form of the same JSON
//! that decodes back byte-identical ([`lp_json_pack`], `lp-base/lp-json-pack`).
//! The per-link choice ([`WireEncoding`], `wire_encoding` module), when to
//! ask for it ([`PackOptIn`], `pack_opt_in` module), and the per-link learned
//! table both ends keep in step (`wire_stream` module) are documented on
//! those items; see `docs/adr/2026-09-24-json-pack-wire-encoding.md` and
//! `docs/adr/2026-09-25-learned-wire-dictionary.md` for the decisions.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod budget;
pub mod json;
pub mod message;
pub mod messages;
pub mod pack_opt_in;
#[cfg(feature = "ser-write-json")]
pub mod pack_sink;
#[cfg(feature = "ser-write-json")]
pub mod packed_frame;
pub mod project;
pub mod project_command;
pub mod project_inventory;
pub mod project_overlay;
#[cfg(feature = "ser-write-json")]
pub mod ser_write;
pub mod serde_base64;
pub mod server;
pub mod slot;
#[cfg(test)]
mod test_traffic;
pub mod transport_error;
pub mod tree;
pub mod wire_encoding;
pub mod wire_stream;

/// The JSON Pack format a packing board and its host must share; see
/// [`ServerHello::pack_format`].
pub use lp_json_pack::PACK_FORMAT_VERSION;
/// A packed link's learned table: a board (or a host-side stand-in for one)
/// codes against one with [`ser_learned_frame_to`]; [`WireStream`] keeps the
/// host's twin itself.
pub use lp_json_pack::{LearnStore, LearnedTable};
pub use messages::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlProductGeometry,
    ControlProductProbeRequest, ControlProductProbeResult, ControlProductProbeResultHeader,
    GeometryDisplayLayout, KnownRevision, NodeReadQuery, NodeReadSelection, OutputFrameEntry,
    OutputFrameEntryHeader, OutputFrameGeometry, OutputFrameProbeRequest, OutputFrameProbeResult,
    OutputFrameProbeResultHeader, PROJECT_READ_FRAME_MAX_BYTES,
    PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES, PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES,
    PROJECT_READ_PROBE_HEADER_RESERVE_BYTES, PROJECT_READ_RUNTIME_CHUNK_BYTES, ProjectProbeRequest,
    ProjectProbeResult, ProjectProbeResultHeader, ProjectReadEvent, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    ProjectReadResourceEvent, ProjectReadShapeEvent, ProjectRuntimeStatus, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, RenderProductProbeResultHeader,
    ResourcePayloadRead, ResourceReadQuery, ResourceReadResult, RevisionGateRead,
    RevisionGateResult, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus, ShapeReadQuery,
    TimebaseProbeRequest, TimebaseProbeResult, WireBindingDirection, WireBindingEndpoint,
    WireBindingGraph, WireBindingGraphRead, WireBindingOrigin, WireBusChannel, WireBusChannelValue,
    WireBusChannelValues, WireCellProjection, WireConsumerPolicy, WireEffectiveBinding,
    WireOutputPlacement, WirePhasorOrigin, WirePhasorReading, WirePhasorRow, WireProjectionOrigin,
    WireProjectionShape, WireScopeRef, WireVisualSpace,
};
pub use messages::{ClientMessage, ClientRequest, Message, ServerMessage};
pub use pack_opt_in::{PACK_OPT_IN_REQUEST_ID, PACK_REASK_INTERVAL_MS, PackOptIn, PackOptInStep};
#[cfg(feature = "ser-write-json")]
pub use pack_sink::PackSink;
#[cfg(feature = "ser-write-json")]
pub use packed_frame::ser_learned_frame_to;
pub use project::{
    NodeRuntimeStatus, WireChannelSampleFormat, WireColorLayout, WireProjectHandle,
    WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
    WireResourceSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
    WireRuntimeBufferPayload, WireTextureFormat, linear16_to_srgb8, srgb8_to_linear16,
};
pub use project_command::{
    WireCreateNodeRequest, WireCreateNodeResponse, WireNodeCommand, WireNodeCommandResponse,
    WirePanelAutoSaveRequest, WirePanelClearRequest, WirePanelCommandResponse,
    WirePanelWriteRequest, WireProjectCommand, WireProjectCommandResponse, WireRemoveNodeRequest,
    WireRemoveNodeResponse,
};
pub use project_inventory::{
    WireProjectInventoryReadRequest, WireProjectInventoryReadResponse,
    WireProjectNodeInventoryEntry, WireProjectNodeOrigin,
};
pub use project_overlay::{
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
};
#[cfg(feature = "ser-write-json")]
pub use ser_write::{
    CountingSerWrite, ErasedWriteError, WireWriteError, ser_learned_to, ser_write_json_fnv64,
    ser_write_json_len, ser_write_json_to,
};
pub use server::{
    AccessEntryInfo, AvailableProject, BuildFacts, FAULT_MESSAGE_CAP_BYTES, FAULT_NODES_CAP,
    FaultedNodeWire, FsRequest, FsResponse, HardwareFacts, HardwareIdentity, HeartbeatIdentity,
    HelloAuth, HelloIdentity, LinkCounters, LoadedProject, MemoryStats, ProjectFaultWire,
    SampleStats, ServerConfig, ServerHello, ServerMsgBody, WIRE_PROTO_VERSION,
};
pub use slot::{
    WireSlotChange, WireSlotData, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot,
    WireSlotRootsSnapshot, build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff,
    snapshot_slot_root, snapshot_slot_shape, wire_slot_data_from_slot_access,
};
pub use transport_error::TransportError;
pub use tree::{WireChildKind, WireEntryState, WireSlotIndex, WireTreeDelta};
pub use wire_encoding::{FRAME_KIND_LEARNED, WireEncoding};
pub use wire_stream::{
    DesyncedFrame, UnpackEvent, UnpackedFrame, WIRE_STREAM_MAX_FRAME, WireChunk, WireForm,
    WireFrame, WireStream, WireUnpacker,
};

/// Canonical project-read message envelope.
pub type WireMessage = Message;
/// Canonical project-read server message.
pub type WireServerMessage = ServerMessage;
/// Canonical project-read server message body.
pub type WireServerMsgBody = ServerMsgBody;
