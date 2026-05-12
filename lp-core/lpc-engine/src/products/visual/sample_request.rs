//! Direct visual sampling request types.

use alloc::vec::Vec;

/// Normalized shader-space sample point encoded as Q16.16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualSamplePoint {
    pub x_q16: i32,
    pub y_q16: i32,
}

/// Direct sampling request for a visual product.
#[derive(Debug, Clone)]
pub struct VisualSampleBatch {
    pub points: Vec<VisualSamplePoint>,
    pub time_seconds: f32,
}

/// Direct shader sampling request backed by a reusable guest-addressable point buffer.
pub struct VisualSampleBufferRequest<'a> {
    pub points: &'a mut lp_shader::LpsSamplePointBuf,
    pub output_width: u32,
    pub output_height: u32,
    pub time_seconds: f32,
}

/// Caller-owned target for packed RGBA16 sample results.
pub struct VisualSampleTarget<'a> {
    pub samples: &'a mut lp_shader::LpsSampleRgba16Buf,
}
