//! Wire-facing project types (`Wire*` where applicable).

mod resource_sync;
mod wire_project_handle;
mod wire_project_request;
mod wire_slot_watch_specifier;

pub use resource_sync::{
    RenderProductPayloadOptions, RenderProductPayloadRequest, RenderProductPayloadSpecifier,
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, WireChannelSampleFormat,
    WireColorLayout, WireRenderProductKind, WireRenderProductPayload, WireResourceAvailability,
    WireResourceKindSummary, WireResourceMetadataSummary, WireResourceSummary,
    WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
    WireTextureFormat,
};
pub use wire_project_handle::WireProjectHandle;
pub use wire_project_request::{WireNodeStatus, WireProjectRequest};
pub use wire_slot_watch_specifier::{WireNodeSlotRoot, WireSlotRootKind, WireSlotWatchSpecifier};
