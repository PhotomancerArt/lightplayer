//! High-level engine wrapping [`lpvm::LpvmEngine`].

use alloc::format;
use alloc::string::String;

use lps_shared::{LpsModuleSig, LpsType, TextureBuffer, TextureStorageFormat};
use lpvm::AllocError;
use lpvm::LpvmEngine;

use crate::error::LpsError;
use crate::px_shader::LpsPxShader;
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
    /// Validates the `render(vec2 pos)` signature against `output_format`.
    /// Returns `Validation` error if signature mismatch.
    pub fn compile_px(
        &self,
        glsl: &str,
        output_format: TextureStorageFormat,
    ) -> Result<LpsPxShader<E::Module>, LpsError> {
        let naga = lps_frontend::compile(glsl).map_err(|e| LpsError::Parse(format!("{e}")))?;
        let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| LpsError::Lower(format!("{e}")))?;
        drop(naga);

        let render_fn_index = validate_render_sig(&meta, output_format)?;

        let module = self
            .engine
            .compile(&ir, &meta)
            .map_err(|e| LpsError::Compile(format!("{e}")))?;
        LpsPxShader::new(module, meta, output_format, render_fn_index)
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

    /// Access the underlying LPVM engine.
    #[must_use]
    pub fn inner(&self) -> &E {
        &self.engine
    }
}

/// Validate the `render` function signature against the output format.
fn validate_render_sig(
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
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
