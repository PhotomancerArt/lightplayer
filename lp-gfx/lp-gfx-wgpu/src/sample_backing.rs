//! CPU-resident sample buffers on the GPU backend.
//!
//! Sample points/outputs are host-side vectors: points are tiny (thousands
//! of Q16.16 pairs, rewritten by the engine each tick) and outputs are the
//! quantized results of the sample pass's readback, so neither earns a
//! persistent GPU allocation. `LpShader::sample_rgba16` uploads the point
//! vector into the pass's vertex buffer per call and quantizes the readback
//! into the out vector (see [`crate::sample_pass`]).

use lp_gfx::{GfxError, SampleOutHandle, SamplePointsHandle};

use crate::texture_backing::foreign_handle;

/// Backing for [`SamplePointsHandle`]: `count × 2` Q16.16 coordinates.
pub(crate) struct CpuSamplePoints(pub(crate) Vec<i32>);

/// Backing for [`SampleOutHandle`]: `count × 4` RGBA16 channels.
pub(crate) struct CpuSampleOut(pub(crate) Vec<u16>);

pub(crate) fn sample_points(handle: &SamplePointsHandle) -> Result<&CpuSamplePoints, GfxError> {
    handle
        .backing()
        .downcast_ref::<CpuSamplePoints>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_points_mut(
    handle: &mut SamplePointsHandle,
) -> Result<&mut CpuSamplePoints, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<CpuSamplePoints>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_out(handle: &SampleOutHandle) -> Result<&CpuSampleOut, GfxError> {
    handle
        .backing()
        .downcast_ref::<CpuSampleOut>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_out_mut(handle: &mut SampleOutHandle) -> Result<&mut CpuSampleOut, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<CpuSampleOut>()
        .ok_or_else(foreign_handle)
}
