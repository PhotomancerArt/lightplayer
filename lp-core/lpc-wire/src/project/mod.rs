//! Wire-facing project types (`Wire*` where applicable).

mod resource_sync;
mod srgb8_sample_codec;
mod wire_project_handle;

pub use lpc_model::NodeRuntimeStatus;
pub use resource_sync::{
    WireChannelSampleFormat, WireColorLayout, WireResourceAvailability, WireResourceKindSummary,
    WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind,
    WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload, WireTextureFormat,
};
pub use srgb8_sample_codec::{linear16_to_srgb8, srgb8_to_linear16};
pub use wire_project_handle::WireProjectHandle;
