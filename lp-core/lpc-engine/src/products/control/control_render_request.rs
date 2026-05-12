//! Request shape for materializing a [`ControlProduct`](lpc_model::ControlProduct).

use lpc_model::ControlExtent;

/// Native sample format for output-owned control buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSampleFormat {
    Unorm16,
}

/// Request for rendering logical control samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlRenderRequest {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
}

impl ControlRenderRequest {
    #[must_use]
    pub const fn unorm16(extent: ControlExtent) -> Self {
        Self {
            extent,
            sample_format: ControlSampleFormat::Unorm16,
        }
    }
}
