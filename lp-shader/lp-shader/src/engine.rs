//! High-level engine wrapping [`lpvm::LpvmEngine`].

use alloc::format;

use lpir::{CompilerConfig, LpirModule};
use lps_shared::{LpsModuleSig, LpsType, TextureBuffer, TextureStorageFormat};
use lpvm::AllocError;
use lpvm::LpvmEngine;

use crate::compile_compute_desc::CompileComputeDesc;
use crate::compile_job::{ShaderCompileBudget, ShaderCompileJob, ShaderCompileStepResult};
use crate::compile_px_desc::{CompilePxDesc, TextureBindingSpecs};
use crate::compute_abi::{validate_compute_abi, validate_compute_tick_sig};
use crate::compute_shader::LpsComputeShader;
use crate::entry_space::{RenderEntry, ShaderEntrySpace};
use crate::error::LpsError;
use crate::px_shader::LpsPxShader;
use crate::sample_buf::{LpsSamplePointBuf, LpsSampleRgba16Buf};
use crate::texture_buf::LpsTextureBuf;

/// Shader compilation and shared-memory texture allocation.
pub struct LpsEngine<E: LpvmEngine> {
    engine: E,
}

impl<E: LpvmEngine> LpsEngine<E> {
    #[must_use]
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    /// Compile GLSL into a pixel shader.
    ///
    /// `config` is passed to the LPVM backend on compile ([`LpvmEngine::compile_with_config`]).
    ///
    /// Validates the declared entry (`render_2d(vec2)` by default; see
    /// [`crate::ShaderEntrySpace`] and [`CompilePxDesc::space`]) against
    /// `output_format`. Returns `Validation` error if signature mismatch.
    ///
    /// Also synthesises a format-specific `__render_texture_<format>` function
    /// (see [`crate::synth::render_texture`]); it is recorded in
    /// [`LpsModuleSig::functions`] with [`lps_shared::LpsFnKind::Synthetic`].
    /// Discover it with
    /// `meta().functions.iter().filter(|f| f.kind == lps_shared::LpsFnKind::Synthetic)`.
    pub fn compile_px(
        &self,
        glsl: &str,
        output_format: TextureStorageFormat,
        config: &CompilerConfig,
        frontend: crate::ShaderFrontend,
    ) -> Result<LpsPxShader, LpsError>
    where
        E::Module: 'static,
    {
        let desc = CompilePxDesc::new(glsl, output_format, config.clone(), frontend);
        self.compile_px_desc(desc)
    }

    /// Compile GLSL into a pixel shader using a [`CompilePxDesc`].
    ///
    /// `desc.textures` must list exactly one entry per GLSL `uniform sampler2D`
    /// declared in the source (and no extra keys).
    pub fn compile_px_desc(&self, desc: CompilePxDesc<'_>) -> Result<LpsPxShader, LpsError>
    where
        E::Module: 'static,
    {
        let mut job = self.start_compile_px_job(desc);
        loop {
            match job.step(ShaderCompileBudget::default()) {
                ShaderCompileStepResult::Pending => {}
                ShaderCompileStepResult::Finished(shader) => return Ok(shader),
                ShaderCompileStepResult::Failed(err) => return Err(err),
            }
        }
    }

    pub fn start_compile_px_job<'a>(
        &'a self,
        desc: CompilePxDesc<'a>,
    ) -> ShaderCompileJob<'a, 'a, E>
    where
        E::Module: 'static,
    {
        ShaderCompileJob::new(&self.engine, desc)
    }

    /// Compile GLSL into a serial compute shader.
    pub fn compile_compute_desc(
        &self,
        desc: CompileComputeDesc<'_>,
    ) -> Result<LpsComputeShader, LpsError>
    where
        E::Module: 'static,
    {
        let CompileComputeDesc {
            glsl,
            compiler_config,
            abi,
            float_mode,
        } = desc;

        let lower_options = lps_glsl::CompileOptions {
            texture_specs: Default::default(),
            texel_fetch_bounds: compiler_config.texture.texel_fetch_bounds,
        };
        let output =
            lps_glsl::compile(glsl, &lower_options).map_err(|e| LpsError::Parse(e.render(glsl)))?;
        let (ir, meta) = (output.ir, output.meta);

        let tick_fn_index = validate_compute_tick_sig(&meta)?;
        validate_compute_abi(&meta, &abi)?;
        if !self.engine.supports_float_mode(float_mode) {
            return Err(LpsError::Validation(format!(
                "compute shader requested float_mode={}, \
                 which this LPVM engine does not compile",
                float_mode.as_str()
            )));
        }
        let module = self
            .engine
            .compile_with_params(
                &ir,
                &meta,
                &lpvm::LpvmCompileParams {
                    config: compiler_config,
                    float_mode,
                },
            )
            .map_err(|e| LpsError::Compile(format!("{e}")))?;
        LpsComputeShader::new(module, meta, &ir, tick_fn_index)
    }

    /// Allocate a texture in the engine's shared memory.
    ///
    /// The buffer is zeroed and guest-addressable.
    pub fn alloc_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
    ) -> Result<LpsTextureBuf, AllocError> {
        let bpp = format.bytes_per_pixel();
        let size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|s| s.checked_mul(bpp))
            .ok_or(AllocError::InvalidSize)?;
        let align = 4;
        let buffer = self.engine.memory().alloc(size, align)?;
        let mut out = LpsTextureBuf::new(buffer, width, height, format);
        out.data_mut().fill(0);
        Ok(out)
    }

    /// Free a texture previously allocated by [`Self::alloc_texture`].
    ///
    /// Backends with bump-style memory may not be able to reuse this memory, but
    /// native embedded backends can return it to the heap. Callers should pair
    /// transient render-target allocations with this method rather than relying
    /// on [`LpsTextureBuf`] drop semantics.
    pub fn free_texture(&self, texture: LpsTextureBuf) {
        self.engine.memory().free(texture.buffer());
    }

    pub fn alloc_sample_points(&self, count: u32) -> Result<LpsSamplePointBuf, AllocError> {
        let size = (count as usize)
            .checked_mul(8)
            .ok_or(AllocError::InvalidSize)?;
        let buffer = self.engine.memory().alloc(size, 4)?;
        let mut out = LpsSamplePointBuf::new(buffer, count);
        out.data_mut().fill(0);
        Ok(out)
    }

    pub fn alloc_sample_rgba16(&self, count: u32) -> Result<LpsSampleRgba16Buf, AllocError> {
        let size = (count as usize)
            .checked_mul(8)
            .ok_or(AllocError::InvalidSize)?;
        let buffer = self.engine.memory().alloc(size, 4)?;
        let mut out = LpsSampleRgba16Buf::new(buffer, count);
        out.data_mut().fill(0);
        Ok(out)
    }

    pub fn free_sample_points(&self, buffer: LpsSamplePointBuf) {
        self.engine.memory().free(buffer.buffer());
    }

    pub fn free_sample_rgba16(&self, buffer: LpsSampleRgba16Buf) {
        self.engine.memory().free(buffer.buffer());
    }

    /// Access the underlying LPVM engine.
    #[must_use]
    pub fn inner(&self) -> &E {
        &self.engine
    }
}

#[cfg(feature = "naga")]
pub(crate) fn lower_glsl_with_naga(
    glsl: &str,
    textures: &TextureBindingSpecs,
    compiler_config: &CompilerConfig,
) -> Result<(LpirModule, LpsModuleSig), LpsError> {
    let naga = lps_frontend::compile(glsl).map_err(|e| LpsError::Parse(format!("{e}")))?;
    let lower_options = lps_frontend::LowerOptions {
        texture_specs: textures.clone(),
        texel_fetch_bounds: compiler_config.texture.texel_fetch_bounds,
    };
    lps_frontend::lower_with_options(&naga, &lower_options)
        .map_err(|e| LpsError::Lower(format!("{e}")))
}

#[cfg(not(feature = "naga"))]
pub(crate) fn lower_glsl_with_naga(
    _glsl: &str,
    _textures: &TextureBindingSpecs,
    _compiler_config: &CompilerConfig,
) -> Result<(LpirModule, LpsModuleSig), LpsError> {
    Err(LpsError::Validation(alloc::string::String::from(
        "naga frontend was not built into this binary",
    )))
}

/// Validate the declared entry against the source and the output format.
///
/// Declaration-driven (dimensionality plan D19): `space` says which entry
/// must exist, and the source is checked against that answer rather than
/// searched for whatever it happens to define. The four refusals are the
/// D1 cross-validation error class — each names *both* sides:
///
/// - a function named `render` (the pre-v6 entry) — hard error with the
///   rename, whatever the declaration says;
/// - both entries defined — multi-entry is deliberately not implemented;
/// - the *other* space's entry defined — declaration ↔ entry mismatch;
/// - neither defined — the declared entry is missing.
///
/// The declaration's `SpaceAnswer` cells (how a source answers the opposite
/// dimension) are a *sampling*-side decision and deliberately play no part
/// here.
pub(crate) fn validate_render_sig(
    meta: &LpsModuleSig,
    output_format: TextureStorageFormat,
    space: ShaderEntrySpace,
) -> Result<RenderEntry, LpsError> {
    let index_of = |name: &str| meta.functions.iter().position(|f| f.name == name);

    if index_of("render").is_some() {
        return Err(LpsError::Validation(format!(
            "`render` is no longer a shader entry point: rename `render` to `{}` \
             (a 2D shader's entry is `{}`, a 1D shader's is `{}`); \
             projects saved before v6 are migrated automatically",
            space.entry_name(),
            ShaderEntrySpace::TwoD.entry_signature(),
            ShaderEntrySpace::OneD.entry_signature(),
        )));
    }

    let declared = index_of(space.entry_name());
    let other = index_of(space.other().entry_name());

    let index = match (declared, other) {
        (Some(_), Some(_)) => {
            return Err(LpsError::Validation(format!(
                "multiple entries are not supported yet: this shader defines both \
                 `{}` and `{}` — keep the one matching its declared space ({})",
                ShaderEntrySpace::OneD.entry_name(),
                ShaderEntrySpace::TwoD.entry_name(),
                space.label(),
            )));
        }
        (None, Some(_)) => {
            return Err(LpsError::Validation(format!(
                "declared {} but defines `{}`: a {}-declared shader's entry is `{}` — \
                 change the declared space to {} or rename the entry to `{}`",
                space.label(),
                space.other().entry_name(),
                space.label(),
                space.entry_signature(),
                space.other().label(),
                space.entry_name(),
            )));
        }
        (None, None) => {
            return Err(LpsError::Validation(format!(
                "no `{}` function found: a {}-declared shader must define `{}`",
                space.entry_name(),
                space.label(),
                space.entry_signature(),
            )));
        }
        (Some(index), None) => index,
    };
    let sig = &meta.functions[index];

    // Check parameter: exactly one coordinate, of the declared space's type.
    let expected_param = match space {
        ShaderEntrySpace::TwoD => LpsType::Vec2,
        ShaderEntrySpace::OneD => LpsType::Float,
    };
    if sig.parameters.len() != 1 {
        return Err(LpsError::Validation(format!(
            "`{}` must take exactly 1 parameter ({}), found {}",
            space.entry_name(),
            match space {
                ShaderEntrySpace::TwoD => "vec2 pos",
                ShaderEntrySpace::OneD => "float pos",
            },
            sig.parameters.len()
        )));
    }
    if sig.parameters[0].ty != expected_param {
        return Err(LpsError::Validation(format!(
            "`{}` parameter must be {}, found {:?}",
            space.entry_name(),
            match space {
                ShaderEntrySpace::TwoD => "vec2",
                ShaderEntrySpace::OneD => "float",
            },
            sig.parameters[0].ty
        )));
    }

    // Check return type matches output format
    let expected_return = expected_return_type(output_format);
    if sig.return_type != expected_return {
        return Err(LpsError::Validation(format!(
            "`{}` return type must be {:?} for format {:?}, found {:?}",
            space.entry_name(),
            expected_return,
            output_format,
            sig.return_type
        )));
    }

    Ok(RenderEntry { space, index })
}

/// Map output format to expected return type.
fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::R16Unorm => LpsType::Float,
        TextureStorageFormat::Rgb16Unorm => LpsType::Vec3,
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
