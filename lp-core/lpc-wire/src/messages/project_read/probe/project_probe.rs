//! Top-level project probe variants.

use alloc::vec::Vec;

use super::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlProductProbeRequest,
    ControlProductProbeResult, RenderProductProbeRequest, RenderProductProbeResult,
};

/// Request-scoped diagnostic work attached to a project read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectProbeRequest {
    RenderProduct(RenderProductProbeRequest),
    ControlProduct(ControlProductProbeRequest),
    BindingGraph(BindingGraphProbeRequest),
    // Future: ShaderPixel(ShaderPixelProbeRequest),
    // Future: ShaderTrace(ShaderTraceProbeRequest),
    // Future: ControlBuffer(ControlBufferProbeRequest),
    // Future: Filesystem(FilesystemProbeRequest),
    // Future: Io(IoProbeRequest),
}

/// Result aligned with one [`ProjectProbeRequest`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectProbeResult {
    RenderProduct(RenderProductProbeResult),
    ControlProduct(ControlProductProbeResult),
    BindingGraph(BindingGraphProbeResult),
    // Future: ShaderPixel(ShaderPixelProbeResult),
    // Future: ShaderTrace(ShaderTraceProbeResult),
    // Future: ControlBuffer(ControlBufferProbeResult),
    // Future: Filesystem(FilesystemProbeResult),
    // Future: Io(IoProbeResult),
}

/// A [`ProjectProbeResult`] with its bulk byte payload removed.
///
/// Probe results whose encoded form exceeds the streaming budget are split into
/// a header (this type) plus their bulk `bytes`, which stream as bounded chunks
/// keyed by the enclosing probe index (`ProjectReadProbeEvent::ResultBegin` /
/// `ResultBytes` / `ResultEnd`). The header carries every field of the original
/// result except the bulk bytes; [`ProjectProbeResultHeader::into_result`]
/// reattaches the reassembled bytes to recover the full [`ProjectProbeResult`].
///
/// Only the two bulk-bearing variants are representable here
/// ([`RenderProductProbeResult::Texture`] and
/// [`ControlProductProbeResult::Preview`]); every other probe result is small
/// and always travels whole in `ProjectReadProbeEvent::Result`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectProbeResultHeader {
    RenderProduct(super::RenderProductProbeResultHeader),
    ControlProduct(super::ControlProductProbeResultHeader),
}

impl ProjectProbeResult {
    /// Split a bulk-bearing result into its header and bulk bytes.
    ///
    /// Returns `Ok((header, bytes))` for the two variants that carry a bulk
    /// `bytes` payload ([`RenderProductProbeResult::Texture`],
    /// [`ControlProductProbeResult::Preview`]) and `Err(self)` — the unmodified
    /// result — for every other variant, which is small enough to always send
    /// whole. The producer uses the returned `bytes` length to decide whether
    /// chunking is worth it, handing the result straight back on `Err`.
    pub fn into_chunked_parts(self) -> Result<(ProjectProbeResultHeader, Vec<u8>), Self> {
        match self {
            Self::RenderProduct(result) => match result.into_chunked_parts() {
                Ok((header, bytes)) => Ok((ProjectProbeResultHeader::RenderProduct(header), bytes)),
                Err(result) => Err(Self::RenderProduct(result)),
            },
            Self::ControlProduct(result) => match result.into_chunked_parts() {
                Ok((header, bytes)) => {
                    Ok((ProjectProbeResultHeader::ControlProduct(header), bytes))
                }
                Err(result) => Err(Self::ControlProduct(result)),
            },
            Self::BindingGraph(_) => Err(self),
        }
    }
}

impl ProjectProbeResultHeader {
    /// Reattach reassembled bulk `bytes` to recover the full result.
    #[must_use]
    pub fn into_result(self, bytes: Vec<u8>) -> ProjectProbeResult {
        match self {
            Self::RenderProduct(header) => {
                ProjectProbeResult::RenderProduct(header.into_result(bytes))
            }
            Self::ControlProduct(header) => {
                ProjectProbeResult::ControlProduct(header.into_result(bytes))
            }
        }
    }
}

impl ProjectProbeRequest {
    #[cfg(test)]
    pub(crate) fn unsupported_example_for_test() -> Self {
        use lpc_model::{ControlExtent, ControlProduct, NodeId};

        Self::ControlProduct(ControlProductProbeRequest {
            product: ControlProduct::new(NodeId::new(1), 0, ControlExtent::new(1, 3)),
            sample_format: crate::WireChannelSampleFormat::U16,
            display_layout: super::ControlDisplayLayoutRead::None,
        })
    }
}
