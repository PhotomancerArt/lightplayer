//! Mutable output-owned target for control rendering.

use lpc_model::ControlExtent;

use super::ControlSampleFormat;

/// Output-owned mutable target for a control materialization request.
pub struct ControlRenderTarget<'a> {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
    pub samples: &'a mut [u16],
}

impl<'a> ControlRenderTarget<'a> {
    #[must_use]
    pub fn new(
        extent: ControlExtent,
        sample_format: ControlSampleFormat,
        samples: &'a mut [u16],
    ) -> Self {
        Self {
            extent,
            sample_format,
            samples,
        }
    }
}
