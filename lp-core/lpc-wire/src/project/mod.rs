//! Wire-facing project types (`Wire*` where applicable).

mod read;
mod resource_sync;
mod wire_project_handle;
mod wire_project_request;

pub use read::{
    ExplainSlotProbeRequest, ExplainSlotProbeResult, NodeReadQuery, NodeReadResult,
    NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResult, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    ResourceReadResult, ShapeReadQuery, ShapeReadResult, SlotExplanation,
};
pub use resource_sync::{
    WireChannelSampleFormat, WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
};
pub use wire_project_handle::WireProjectHandle;
pub use wire_project_request::{WireNodeStatus, WireProjectRequest};
