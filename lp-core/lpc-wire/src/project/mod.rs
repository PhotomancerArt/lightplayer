//! Wire-facing project types (`Wire*` where applicable).

mod resource_sync;
mod wire_project_handle;
mod wire_project_request;
mod wire_slot_watch_specifier;

pub use resource_sync::{
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, VisualProductPayloadOptions,
    VisualProductPayloadRequest, VisualProductPayloadSpecifier, WireChannelSampleFormat,
    WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
    WireVisualProductKind, WireVisualProductPayload,
};
pub use wire_project_handle::WireProjectHandle;
pub use wire_project_request::{WireNodeStatus, WireProjectRequest};
pub use wire_slot_watch_specifier::{WireNodeSlotRoot, WireSlotRootKind, WireSlotWatchSpecifier};
