//! Resource summary / payload types for stateless project reads.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::NodeId;
use lpc_model::project::Revision;
use lpc_model::resource::ResourceRef;
use serde::{Deserialize, Serialize};

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonValue, JsonWriterError};
use crate::json::streaming_base64::write_base64_value;

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

/// Texture-ish pixel layout for summaries and payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireTextureFormat {
    Rgba16,
    Rgb8,
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

/// Write a runtime-buffer payload JSON object without allocating encoded bytes.
///
/// The object has the same shape as [`WireRuntimeBufferPayload`]. Metadata and
/// small scalar fields still use the serde bridge, but `bytes` are base64
/// encoded directly into the supplied JSON value.
pub fn write_runtime_buffer_payload_json<W>(
    value: JsonValue<'_, W>,
    payload: &WireRuntimeBufferPayload,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    let mut object = value.object()?;
    object.prop("ref")?.serde(&payload.resource_ref)?;
    object.prop("revision")?.serde(&payload.revision)?;
    object.prop("metadata")?.serde(&payload.metadata)?;
    write_base64_value(object.prop("bytes")?, &payload.bytes)?;
    object.finish()
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
    use crate::json::json_writer::JsonWriter;
    use alloc::vec;
    use lpc_model::RuntimeBufferId;
    use lpc_model::resource::ResourceDomain;

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

    #[test]
    fn runtime_buffer_payload_streams_base64_bytes() {
        let payload = WireRuntimeBufferPayload {
            resource_ref: ResourceRef {
                domain: ResourceDomain::RuntimeBuffer,
                id: 3,
            },
            revision: Revision::new(2),
            metadata: WireRuntimeBufferMetadataPayload::OutputChannels {
                channels: 3,
                sample_format: WireChannelSampleFormat::U16,
            },
            bytes: vec![0u8, 1, 2, 253, 254, 255],
        };
        let mut out = Vec::new();
        let mut writer = JsonWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        write_runtime_buffer_payload_json(object.prop("payload").unwrap(), &payload).unwrap();
        object.finish().unwrap();
        let json = core::str::from_utf8(&out).unwrap();
        let wrapped: serde_json::Value = serde_json::from_str(json).unwrap();
        let decoded: WireRuntimeBufferPayload =
            serde_json::from_value(wrapped["payload"].clone()).unwrap();

        assert_eq!(decoded, payload);
        assert_eq!(wrapped["payload"]["bytes"], "AAEC/f7/");
    }
}
