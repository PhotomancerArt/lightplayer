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

    /// The [`ShaderSemantics`] tier this backend executes natively.
    ///
    /// Tier selection happens once, when the host constructs the backend
    /// (fidelity-tiers ADR); visual render paths align their compile
    /// requests with the selected backend by asking it — the honor-or-fail
    /// contract on [`Self::compile_shader`] stays intact (a mismatched
    /// explicit request still errors, never silently substitutes).
    fn native_semantics(&self) -> crate::ShaderSemantics {
        crate::ShaderSemantics::Q32
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

    /// Blend two same-shape RGBA16 textures into `target`:
    /// `target = previous × (1 − alpha) + active × alpha` per channel
    /// (`alpha` clamped to `[0, 1]`, result rounded to the unorm16 grid).
    ///
    /// This is the first member of the **GPU-resident texture-op family**:
    /// operations on render products belong behind this trait so the data
    /// never leaves the GPU on accelerated backends. [`Self::read_back`] is
    /// reserved for sinks that inherently need bytes (fixture sampling, wire
    /// probes) — never for transforms. See the crate README.
    fn blend_textures(
        &self,
        previous: &TextureHandle,
        active: &TextureHandle,
        alpha: f32,
        target: &mut TextureHandle,
    ) -> Result<(), GfxError>;

    /// Read a texture back as owned CPU bytes.
    ///
    /// For sinks that inherently need bytes (fixture sampling, wire probes).
    /// Transforms on render products belong behind GPU-resident ops like
    /// [`Self::blend_textures`] instead — see the crate README doctrine.
    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError>;

    /// Whether [`Self::read_back`] can service requests on this backend.
    ///
    /// CPU backends keep textures host-resident and always answer `true`
    /// (the default). The browser GPU tier answers `false`: readback would
    /// require blocking on an async buffer map, so render products stay
    /// GPU-resident and byte-needing consumers must run on the CPU tier
    /// (`docs/adr/2026-07-09-preview-fidelity-tiers.md`). Render paths use
    /// this to decide between materializing byte-backed texture products and
    /// returning handle-carrying (GPU-resident) ones — an explicit
    /// capability, never an error-sniffing fallback.
    fn supports_read_back(&self) -> bool {
        true
    }

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
