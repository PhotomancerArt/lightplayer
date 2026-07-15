//! Visual sampling request types.

use alloc::vec::Vec;

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
/// `points` are shader pixel-space Q16.16 coordinates. `output_width` and
/// `output_height` define the shader `outputSize` uniform for those points.
pub struct VisualSampleBufferRequest<'a> {
    pub points: &'a mut lp_gfx::SamplePointsHandle,
    pub output_width: u32,
    pub output_height: u32,
    pub time_seconds: f32,
}

/// Caller-owned target for packed RGBA16 sample results.
pub struct VisualSampleTarget<'a> {
    pub samples: &'a mut lp_gfx::SampleOutHandle,
}
