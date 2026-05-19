//! High-level engine wrapping [`lpvm::LpvmEngine`].

use alloc::format;
use alloc::string::String;

use lpir::{CompilerConfig, LpirModule};
use lps_shared::{LpsModuleSig, LpsType, TextureBuffer, TextureStorageFormat};
use lpvm::AllocError;
use lpvm::LpvmEngine;

use crate::compile_compute_desc::CompileComputeDesc;
use crate::compile_job::{ShaderCompileBudget, ShaderCompileJob, ShaderCompileStepResult};
use crate::compile_px_desc::{CompilePxDesc, TextureBindingSpecs};
use crate::compute_abi::{validate_compute_abi, validate_compute_tick_sig};
use crate::compute_shader::LpsComputeShader;
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
    /// Validates the `render(vec2 pos)` signature against `output_format`.
    /// Returns `Validation` error if signature mismatch.
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
    ) -> Result<LpsPxShader, LpsError>
    where
        E::Module: 'static,
    {
        let desc = CompilePxDesc::new(glsl, output_format, config.clone());
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
        let module = self
            .engine
            .compile_with_config(&ir, &meta, &compiler_config)
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
    Err(LpsError::Validation(String::from(
        "naga frontend was not built into this binary",
    )))
}

/// Validate the `render` function signature against the output format.
pub(crate) fn validate_render_sig(
    meta: &LpsModuleSig,
    output_format: TextureStorageFormat,
) -> Result<usize, LpsError> {
    let (index, sig) = meta
        .functions
        .iter()
        .enumerate()
        .find(|(_, f)| f.name == "render")
        .ok_or_else(|| LpsError::Validation(String::from("no `render` function found")))?;

    // Check parameter: exactly one vec2
    if sig.parameters.len() != 1 {
        return Err(LpsError::Validation(format!(
            "`render` must take exactly 1 parameter (vec2 pos), found {}",
            sig.parameters.len()
        )));
    }
    if sig.parameters[0].ty != LpsType::Vec2 {
        return Err(LpsError::Validation(format!(
            "`render` parameter must be vec2, found {:?}",
            sig.parameters[0].ty
        )));
    }

    // Check return type matches output format
    let expected_return = expected_return_type(output_format);
    if sig.return_type != expected_return {
        return Err(LpsError::Validation(format!(
            "`render` return type must be {:?} for format {:?}, found {:?}",
            expected_return, output_format, sig.return_type
        )));
    }

    Ok(index)
}

/// Map output format to expected return type.
fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::R16Unorm => LpsType::Float,
        TextureStorageFormat::Rgb16Unorm => LpsType::Vec3,
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
