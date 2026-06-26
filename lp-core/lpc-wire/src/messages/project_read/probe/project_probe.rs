//! Top-level project probe variants.

use super::{
    ControlProductProbeRequest, ControlProductProbeResult, ExplainSlotProbeRequest,
    ExplainSlotProbeResult, RenderProductProbeRequest, RenderProductProbeResult,
};

/// Request-scoped diagnostic work attached to a project read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectProbeRequest {
    RenderProduct(RenderProductProbeRequest),
    ControlProduct(ControlProductProbeRequest),
    ExplainSlot(ExplainSlotProbeRequest),
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
    ExplainSlot(ExplainSlotProbeResult),
    // Future: ShaderPixel(ShaderPixelProbeResult),
    // Future: ShaderTrace(ShaderTraceProbeResult),
    // Future: ControlBuffer(ControlBufferProbeResult),
    // Future: Filesystem(FilesystemProbeResult),
    // Future: Io(IoProbeResult),
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
