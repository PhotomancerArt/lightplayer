//! The [`LpShader`] trait: a compiled, runnable visual shader.

use alloc::string::String;

use lps_shared::LpsValueF32;

use crate::gfx_error::GfxError;
use crate::sample_out_handle::SampleOutHandle;
use crate::sample_points_handle::SamplePointsHandle;
use crate::texture_handle::TextureHandle;

/// Compile statistics reported by a backend.
pub type ShaderCompileStats = lp_shader::LpsCompileStats;

/// A compiled, runnable visual shader.
///
/// Targets and sample buffers are opaque handles allocated from the same
/// [`crate::LpGraphics`] that compiled this shader; passing a foreign handle
/// yields [`GfxError::Backend`].
pub trait LpShader: Send + Sync {
    /// Run the shader into an RGBA16 render target allocated by
    /// [`crate::LpGraphics::create_render_target`].
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError>;

    /// Run the shader at caller-provided Q16.16 pixel-space points.
    fn sample_rgba16(
        &mut self,
        _points: &mut SamplePointsHandle,
        _out: &mut SampleOutHandle,
        _uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        Err(GfxError::Render(String::from(
            "shader backend does not support direct sampling",
        )))
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        None
    }
}
