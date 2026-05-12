//! Render-product probe.

use crate::project::WireTextureFormat;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{Revision, VisualProduct};

/// Request to materialize a visual product into inspection bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RenderProductProbeRequest {
    pub product: VisualProduct,
    pub width: u32,
    pub height: u32,
    pub format: WireTextureFormat,
}

/// Result of a render-product probe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RenderProductProbeResult {
    Texture {
        product: VisualProduct,
        revision: Revision,
        width: u32,
        height: u32,
        format: WireTextureFormat,
        #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
        #[serde(with = "crate::serde_base64")]
        bytes: Vec<u8>,
    },
    Unsupported {
        reason: String,
    },
    Error {
        message: String,
    },
}
