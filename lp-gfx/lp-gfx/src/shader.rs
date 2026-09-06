//! The [`LpShader`] trait: a compiled, runnable visual shader.

use alloc::string::String;

use lps_shared::LpsValueF32;

use crate::gfx_error::GfxError;
use crate::sample_out_handle::SampleOutHandle;
use crate::sample_points_handle::SamplePointsHandle;
use crate::texture_handle::TextureHandle;

/// Compile statistics reported by a backend.
pub type ShaderCompileStats = lp_shader::LpsCompileStats;
pub use lp_shader::FloatImpl as ShaderFloatImpl;

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

    /// Bind `uniforms` for the sampling calls that follow.
    ///
    /// Binding is the part of a sample that allocates (uniform paths on the
    /// CPU tier, bind groups on the GPU tier), so a consumer streaming a
    /// product in batches binds once per stream and then samples each batch
    /// with [`Self::sample_rgba16_bound`]. The binding stays in effect until
    /// the next `bind_uniforms` or [`Self::render`] call on this shader.
    fn bind_uniforms(&mut self, _uniforms: &LpsValueF32) -> Result<(), GfxError> {
        Err(GfxError::Render(String::from(
            "shader backend does not support direct sampling",
        )))
    }

    /// Run the shader at the first `count` points of `points` with the
    /// uniforms last bound, writing the first `count` results of `out`.
    ///
    /// `count ≤ points.count()` and `count ≤ out.count()`; the tails of both
    /// buffers are untouched. The point packing follows the shader's declared
    /// space (see [`SamplePointsHandle`]).
    fn sample_rgba16_bound(
        &mut self,
        _points: &mut SamplePointsHandle,
        _out: &mut SampleOutHandle,
        _count: u32,
    ) -> Result<(), GfxError> {
        Err(GfxError::Render(String::from(
            "shader backend does not support direct sampling",
        )))
    }

    /// Run the shader at every point of `points`: bind `uniforms`, then
    /// sample the whole buffer. The one-shot form for tests, probes and
    /// parity checks; frame paths stream through the two halves.
    fn sample_rgba16(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let count = points.count();
        if out.count() != count {
            return Err(GfxError::Render(alloc::format!(
                "sample_rgba16: point count {count} does not match output count {}",
                out.count()
            )));
        }
        self.bind_uniforms(uniforms)?;
        self.sample_rgba16_bound(points, out, count)
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        None
    }
}
