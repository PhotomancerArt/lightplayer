//! Resource summary / payload sync specifiers for project sync.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::project::Revision;
use lpc_model::resource::{ResourceRef, RuntimeBufferId};
use serde::{Deserialize, Serialize};

/// Domains requested for project resource summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSummarySpecifier {
    #[default]
    None,
    RuntimeBuffers,
    All,
}

/// Runtime-buffer payloads to include on `GetChanges`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBufferPayloadSpecifier {
    None,
    All,
    ByIds(Vec<RuntimeBufferId>),
}

impl Default for RuntimeBufferPayloadSpecifier {
    fn default() -> Self {
        Self::None
    }
}

/// Classification line in a [`WireResourceSummary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireRuntimeBufferKind {
    Texture,
    FixtureColors,
    OutputChannels,
    Raw,
}

/// Summary kind aligned with [`lpc_model::resource::ResourceDomain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireResourceKindSummary {
    RuntimeBuffer(WireRuntimeBufferKind),
}

/// Texture-ish pixel layout for summaries and payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireTextureFormat {
    Rgba16,
    Rgb8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireColorLayout {
    Rgb8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireChannelSampleFormat {
    U8,
    U16,
}

/// Metadata bundled with resource summaries (no raw bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
pub enum WireResourceAvailability {
    Available,
    Pending,
    NotFound,
    Error(String),
}

/// Store-backed resource summary for list/skeleton UX.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireResourceSummary {
    #[serde(rename = "ref")]
    pub resource_ref: ResourceRef,
    pub revision: Revision,
    pub kind: WireResourceKindSummary,
    pub metadata: WireResourceMetadataSummary,
    pub byte_length_hint: Option<u64>,
    pub availability: WireResourceAvailability,
}

/// Full/native runtime-buffer payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRuntimeBufferPayload {
    #[serde(rename = "ref")]
    pub resource_ref: ResourceRef,
    pub revision: Revision,
    pub metadata: WireRuntimeBufferMetadataPayload,
    #[serde(with = "crate::serde_base64")]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    use alloc::vec;

    #[test]
    fn resource_summary_specifier_round_trips_snake_case() {
        let s = crate::json::to_string(&ResourceSummarySpecifier::All).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&s).unwrap(),
            serde_json::Value::String(String::from("all"))
        );
        let _: ResourceSummarySpecifier = crate::json::from_str(&s).unwrap();
    }

    #[test]
    fn runtime_buffer_payload_by_ids_wire_shape() {
        let spec = RuntimeBufferPayloadSpecifier::ByIds(vec![
            RuntimeBufferId::new(3),
            RuntimeBufferId::new(99),
        ]);
        let j = serde_json::to_string(&spec).unwrap();
        let _: RuntimeBufferPayloadSpecifier = serde_json::from_str(&j).unwrap();
    }
}
