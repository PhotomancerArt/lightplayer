//! Wire-facing project types (`Wire*` where applicable).

mod resource_sync;
mod wire_project_handle;

pub use lpc_model::NodeRuntimeStatus;
pub use resource_sync::{
    WireChannelSampleFormat, WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
};
pub use wire_project_handle::WireProjectHandle;
