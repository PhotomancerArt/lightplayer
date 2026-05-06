//! LightPlayer engine↔client wire model (`Wire*` types where needed).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod json;
pub mod legacy;
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
    RenderProductPayloadOptions, RenderProductPayloadRequest, RenderProductPayloadSpecifier,
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, WireChannelSampleFormat,
    WireColorLayout, WireNodeSpecifier, WireNodeStatus, WireProjectHandle, WireProjectRequest,
    WireRenderProductKind, WireRenderProductPayload, WireResourceAvailability,
    WireResourceKindSummary, WireResourceMetadataSummary, WireResourceSummary,
    WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
    WireTextureFormat,
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
