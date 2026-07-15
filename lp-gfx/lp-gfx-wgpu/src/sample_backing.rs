//! CPU-resident sample buffers on the GPU backend.
//!
//! Sample points/outputs stay host-side vectors until the GPU sample-point
//! pass lands (the `sample_rgba16` milestone); the create/write/read/clear
//! surface works so the engine can allocate through this backend, while
//! `LpShader::sample_rgba16` itself reports the milestone gap.

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
