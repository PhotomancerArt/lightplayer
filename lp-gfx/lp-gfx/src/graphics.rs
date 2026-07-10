//! The [`LpGraphics`] backend trait: shader compilation plus resource
//! allocation and byte transfer for the opaque handles.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::TextureStorageFormat;

use crate::compute_shader::LpComputeShader;
use crate::gfx_error::GfxError;
use crate::sample_out_handle::SampleOutHandle;
use crate::sample_points_handle::SamplePointsHandle;
use crate::shader::LpShader;
use crate::shader_compile_options::ShaderCompileOptions;
use crate::texture_data::TextureData;
use crate::texture_handle::TextureHandle;

/// Compiles GLSL and owns shader resources (textures, sample buffers) for one
/// backend.
///
/// Handles returned by the `create_*` methods are RAII (drop frees) and are
/// only valid with the backend that created them. All texel/sample access
/// crosses this trait as owned bytes — no backend pointers escape.
pub trait LpGraphics: Send + Sync {
    /// Compile GLSL into a runnable visual shader.
    ///
    /// The backend must honor [`ShaderCompileOptions::semantics`] exactly or
    /// fail with [`GfxError::Backend`] — never silently substitute a
    /// different tier.
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError>;

    /// Compile a serial compute shader descriptor.
    ///
    /// `lp-shader` owns the ABI contract while the engine remains responsible
    /// for mapping authored slot shapes to and from that ABI. Compute shaders
    /// stay on the CPU tier permanently; accelerated backends keep this
    /// default.
    fn compile_compute_shader(
        &self,
        _desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        Err(GfxError::Backend(String::from(
            "graphics backend does not support compute shaders",
        )))
    }

    /// Human-readable label for logs (e.g. `lpvm-wasm::rt_wasmtime`).
    fn backend_name(&self) -> &'static str {
        "unknown"
    }

    /// Allocate a zeroed RGBA16 render-target texture for
    /// [`LpShader::render`].
    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError>;

    /// Allocate a texture and upload `texels` into it (the texel-upload path
    /// for CPU-produced content such as fluid frames and baked palettes).
    ///
    /// `texels` must be tightly packed `width × height ×
    /// bytes_per_pixel(format)` little-endian bytes.
    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<TextureHandle, GfxError>;

    /// Upload `texels` into an existing texture (full-texture write; `texels`
    /// length must match the texture).
    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError>;

    /// Zero every texel of `texture`.
    fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError>;

    /// Read a texture back as owned CPU bytes.
    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError>;

    /// Allocate a zeroed buffer of `count` Q16.16 pixel-space sample points.
    fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError>;

    /// Write all `count × 2` Q16.16 point coordinates (`[x0, y0, x1, y1, …]`).
    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError>;

    /// Read all `count × 2` Q16.16 point coordinates back.
    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError>;

    /// Allocate a zeroed buffer for `count` RGBA16 sample results.
    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError>;

    /// Write all `count × 4` RGBA16 channels (`[r0, g0, b0, a0, r1, …]`).
    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError>;

    /// Read all `count × 4` RGBA16 channels back.
    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError>;

    /// Zero every channel of `out`.
    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError>;
}
