//! Resource summary / payload types for stateless project reads.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::NodeId;
use lpc_model::project::Revision;
use lpc_model::resource::ResourceRef;
use serde::{Deserialize, Serialize};

/// Classification line in a [`WireResourceSummary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireRuntimeBufferKind {
    Texture,
    FixtureColors,
    OutputChannels,
    Raw,
}

/// Summary kind aligned with [`lpc_model::resource::ResourceDomain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireResourceKindSummary {
    RuntimeBuffer(WireRuntimeBufferKind),
}

/// Texture-ish pixel layout for summaries, payloads, and diagnostic previews.
///
/// `Rgba16` is LightPlayer's native linear visual format. `Srgb8` is a
/// display-oriented preview format used when exact linear sample fidelity would
/// waste bandwidth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireTextureFormat {
    /// Native linear RGBA, little-endian `u16` channels.
    Rgba16,
    /// Display-encoded RGB8 preview data.
    Srgb8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireColorLayout {
    Rgb8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireChannelSampleFormat {
    U8,
    U16,
}

/// Metadata bundled with resource summaries (no raw bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireResourceMetadataSummary {
    Texture {
        width: u32,
        height: u32,
        format: WireTextureFormat,
    },
    FixtureColors {
        channels: u32,
        layout: WireColorLayout,
    },
    OutputChannels {
        channels: u32,
        sample_format: WireChannelSampleFormat,
    },
    Raw,
}

/// Lifecycle / availability hints for listed resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireResourceAvailability {
    Available,
    Pending,
    NotFound,
    Error(String),
}

/// Store-backed resource summary for list/skeleton UX.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireResourceSummary {
    #[serde(rename = "ref")]
    pub resource_ref: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<NodeId>,
    pub revision: Revision,
    pub kind: WireResourceKindSummary,
    pub metadata: WireResourceMetadataSummary,
    pub byte_length_hint: Option<u64>,
    pub availability: WireResourceAvailability,
}

/// Full/native runtime-buffer payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireRuntimeBufferPayload {
    #[serde(rename = "ref")]
    pub resource_ref: ResourceRef,
    pub revision: Revision,
    pub metadata: WireRuntimeBufferMetadataPayload,
    #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
    #[serde(with = "crate::serde_base64")]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireRuntimeBufferMetadataPayload {
    Texture {
        width: u32,
        height: u32,
        format: WireTextureFormat,
    },
    FixtureColors {
        channels: u32,
        layout: WireColorLayout,
    },
    OutputChannels {
        channels: u32,
        sample_format: WireChannelSampleFormat,
    },
    Raw,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::RuntimeBufferId;

    #[test]
    fn runtime_buffer_payload_round_trips_base64_bytes() {
        let payload = WireRuntimeBufferPayload {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(3)),
            revision: Revision::new(2),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: Vec::from([1u8, 2, 3]),
        };
        let j = serde_json::to_string(&payload).unwrap();
        let back: WireRuntimeBufferPayload = serde_json::from_str(&j).unwrap();
        assert_eq!(back, payload);
    }
}
