//! Compiled visual shader running on an LPVM engine.

use alloc::format;

use lp_gfx::{
    GfxError, LpShader, SampleOutHandle, SamplePointsHandle, ShaderCompileStats, TextureHandle,
};
use lp_shader::{LpsError, LpsPxShader};
use lps_shared::LpsValueF32;

use crate::lpvm_graphics::{sample_out_buf_mut, sample_points_buf_mut, texture_buf_mut};

/// [`LpShader`] over a compiled [`LpsPxShader`] (pixel loop lives in
/// [`LpsPxShader::render_frame`]).
pub struct LpvmShader {
    px: LpsPxShader,
}

impl LpvmShader {
    pub(crate) fn new(px: LpsPxShader) -> Self {
        Self { px }
    }
}

impl LpShader for LpvmShader {
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let buffer = texture_buf_mut(target)?;
        self.px
            .render_frame(uniforms, buffer)
            .map_err(|e| match e {
                // Out-of-fuel stays structured for the engine's typed
                // panic/blame route (lpvm-native fuel ADR).
                LpsError::FuelExhausted(trap) => GfxError::FuelExhausted(trap),
                e => GfxError::Render(format!("render_frame: {e}")),
            })
    }

    fn sample_rgba16(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let point_buffer = sample_points_buf_mut(points)?;
        let out_buffer = sample_out_buf_mut(out)?;
        self.px
            .sample_points_rgba16(uniforms, point_buffer, out_buffer)
            .map_err(|e| match e {
                LpsError::FuelExhausted(trap) => GfxError::FuelExhausted(trap),
                e => GfxError::Render(format!("sample_points_rgba16: {e}")),
            })
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        Some(self.px.compile_stats())
    }
}
