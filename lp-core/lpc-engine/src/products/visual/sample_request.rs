//! Visual sampling request types.

use alloc::vec::Vec;

use super::{ConsumerPolicy, VisualSpace};

/// Texture UV sample point encoded as Q16.16.
///
/// This is for sampling a materialized texture product, not for direct shader execution.
/// Direct shader sampling uses [`lp_gfx::SamplePointsHandle`], whose points are
/// shader pixel-space Q16.16 coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureUvSamplePoint {
    pub u_q16: i32,
    pub v_q16: i32,
}

/// Texture sampling request for a materialized visual product.
#[derive(Debug, Clone)]
pub struct TextureSampleBatch {
    pub points: Vec<TextureUvSamplePoint>,
    pub time_seconds: f32,
}

/// Direct visual sampling request backed by a reusable backend point buffer.
///
/// `points` are shader pixel-space Q16.16 coordinates, packed for `space`:
/// `[x, y]` pairs for [`VisualSpace::TwoD`], tightly packed single `t`
/// words for [`VisualSpace::OneD`] (written through
/// `LpGraphics::write_sample_points_1d`). `output_width` and
/// `output_height` define the shader `outputSize` uniform for those points
/// — `(N, 1)` for a 1D request.
///
/// `space` and `policy` default to `TwoD` / no policy, which is what every
/// consumer sent before spaces existed.
pub struct VisualSampleBufferRequest<'a> {
    pub points: &'a mut lp_gfx::SamplePointsHandle,
    pub output_width: u32,
    pub output_height: u32,
    pub time_seconds: f32,
    /// Space the consumer is asking in, and the lane packing of `points`.
    pub space: VisualSpace,
    /// The consumer's projection policy, honored by the producer when the
    /// spaces disagree.
    pub policy: ConsumerPolicy,
}

/// Caller-owned target for packed RGBA16 sample results.
pub struct VisualSampleTarget<'a> {
    pub samples: &'a mut lp_gfx::SampleOutHandle,
}
