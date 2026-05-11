//! Resource read query/result.

use super::ReadLevel;
use crate::project::{WireResourceSummary, WireRuntimeBufferPayload};
use alloc::vec::Vec;
use lpc_model::ResourceRef;

/// Resource payload bytes to include in a read response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourcePayloadRead {
    None,
    All,
    ByRefs(Vec<ResourceRef>),
}

impl Default for ResourcePayloadRead {
    fn default() -> Self {
        Self::None
    }
}

/// Request resource summaries and optional payload bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ResourceReadQuery {
    pub level: ReadLevel,
    #[serde(default)]
    pub payloads: ResourcePayloadRead,
}

impl Default for ResourceReadQuery {
    fn default() -> Self {
        Self {
            level: ReadLevel::Summary,
            payloads: ResourcePayloadRead::None,
        }
    }
}

/// Resource read result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ResourceReadResult {
    pub level: ReadLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summaries: Vec<WireResourceSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_buffer_payloads: Vec<WireRuntimeBufferPayload>,
}
