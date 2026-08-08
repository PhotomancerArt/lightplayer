//! The [`LpGraphics`] backend trait: shader compilation plus resource
//! allocation and byte transfer for the opaque handles.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{LpsValueF32, TextureStorageFormat};

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

    /// The [`ShaderSemantics`] tier this backend runs when the *author* asked
    /// for Float — the shader node's `float_mode` slot set to `Float`.
    ///
    /// The sibling of [`Self::native_semantics`], which answers the same
    /// question for `Fixed`. Two methods rather than one mapping function
    /// because the answer is a per-backend product decision in both
    /// directions, stated once where the backend is defined.
    ///
    /// The default is `native_semantics()` — correct for any backend with a
    /// single tier, and deliberately so for two of them: the GPU tier runs
    /// IEEE f32 whichever mode was authored (its documented latitude, see
    /// `docs/adr/2026-07-09-preview-fidelity-tiers.md`), and
    /// [`crate::NullGraphics`] compiles nothing at all, so its answer is
    /// unreachable. A backend that implements both tiers overrides.
    ///
    /// Answering here does **not** promise the compile succeeds: a backend
    /// whose linked build cannot emit the tier still fails
    /// [`Self::compile_shader`] with [`GfxError::Backend`]. That is the
    /// never-silent-fallback rule, not a gap — a board without the float
    /// backend linked must say so rather than quietly render Fixed.
    fn float_semantics(&self) -> crate::ShaderSemantics {
        self.native_semantics()
    }

    /// The GLSL → LPIR frontend this backend compiles visual shaders with.
    ///
    /// Like [`Self::native_semantics`], this is a product decision the host
    /// makes once when it constructs the backend — never a `cfg!`/feature
    /// default, which would let Cargo feature unification change compile
    /// behavior with the build graph. Render paths fill
    /// [`ShaderCompileOptions::frontend`] by asking the backend. Deliberately
    /// has no default implementation: every backend must state its choice.
    fn glsl_frontend(&self) -> lp_shader::ShaderFrontend;

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

    /// The uniform-tree value that binds `texture` to a `sampler2D` uniform.
    ///
    /// The engine currency for a texture input is `LpsValueF32::Texture2D`,
    /// but what its descriptor's `ptr` lane *means* is the backend's own
    /// business: a guest pointer on the CPU tier, a registry id on the GPU
    /// tier. Only the backend that allocated the handle can answer, which is
    /// why this is on the trait rather than something a caller assembles
    /// from [`TextureHandle`]'s public dimensions.
    ///
    /// The value stays valid as long as `texture` is alive and is not
    /// reallocated; [`Self::write_texture`] deliberately does not invalidate
    /// it, which is what lets a bake cache refresh a strip in place instead
    /// of reallocating per frame.
    ///
    /// Defaulted to a refusal rather than a stub: a backend that cannot bind
    /// textures must say so on the uniform that needs one, not render
    /// something wrong. See `docs/design/lp-shader-texture-access.md`.
    fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        let _ = texture;
        Err(GfxError::Backend(String::from(
            "backend does not bind textures to uniforms",
        )))
    }

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
    ///
    /// This is the 2D packing. A 1D-declared shader consumes tightly packed
    /// single words (`[t0, t1, …]`) — the buffer stays pair-sized, so a 1D
    /// writer needs a lane-aware entry point here rather than this
    /// full-slice one (space-tagged sample requests, P4 of the
    /// dimensionality plan).
    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError>;

    /// Write all `count` Q16.16 coordinates of a **1-lane** batch
    /// (`[t0, t1, …]`), the packing a `render_1d` shader consumes.
    ///
    /// The allocation stays pair-sized (see [`SamplePointsHandle`]), so the
    /// `t` words fill the first `count` words and the tail is zeroed slack.
    /// This is a thin wrapper over [`Self::write_sample_points`] rather than
    /// a new backend surface: every backend stores one flat `i32` buffer, so
    /// zero-padding here is exactly the contract
    /// `lp_shader::synth::render_samples` reads. Backends do not override it.
    fn write_sample_points_1d(
        &self,
        points: &mut SamplePointsHandle,
        t_q16: &[i32],
    ) -> Result<(), GfxError> {
        let count = points.count() as usize;
        if t_q16.len() != count {
            return Err(GfxError::Backend(alloc::format!(
                "1D sample point coordinates: buffer holds {count} points, got {}",
                t_q16.len()
            )));
        }
        let mut padded = alloc::vec![0i32; count * 2];
        padded[..count].copy_from_slice(t_q16);
        self.write_sample_points(points, &padded)
    }

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
