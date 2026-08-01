//! Request shape for materializing a [`ControlProduct`](lpc_model::ControlProduct).

use lpc_model::ControlExtent;
use lpc_wire::ControlColorPolicy;

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
    /// Which color pipeline produces the samples (D2, a viewer property):
    /// [`ControlColorPolicy::Wire`] is the device pipeline (brightness →
    /// optional gamma → color-order permutation) and the only policy real
    /// output sinks use; [`ControlColorPolicy::Display`] forks at the
    /// producer's processing point (brightness applied, gamma and color
    /// order skipped) for human-facing previews.
    pub color_policy: ControlColorPolicy,
}

impl ControlRenderRequest {
    /// Wire-policy unorm16 request: the native output path's shape.
    #[must_use]
    pub const fn unorm16(extent: ControlExtent) -> Self {
        Self::unorm16_with_policy(extent, ControlColorPolicy::Wire)
    }

    /// Unorm16 request with an explicit color policy (preview probes).
    #[must_use]
    pub const fn unorm16_with_policy(
        extent: ControlExtent,
        color_policy: ControlColorPolicy,
    ) -> Self {
        Self {
            extent,
            sample_format: ControlSampleFormat::Unorm16,
            color_policy,
        }
    }
}
