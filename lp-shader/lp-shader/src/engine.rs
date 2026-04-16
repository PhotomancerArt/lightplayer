//! High-level engine wrapping [`lpvm::LpvmEngine`].

use alloc::format;

use lps_shared::{TextureBuffer, TextureStorageFormat};
use lpvm::AllocError;
use lpvm::LpvmEngine;

use crate::error::LpsError;
use crate::frag_shader::LpsFragShader;
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

    /// Compile GLSL into a fragment shader.
    ///
    /// `output_format` is recorded for future specialized codegen (roadmap).
    pub fn compile_frag(
        &self,
        glsl: &str,
        output_format: TextureStorageFormat,
    ) -> Result<LpsFragShader<E::Module>, LpsError> {
        let naga = lps_frontend::compile(glsl).map_err(|e| LpsError::Parse(format!("{e}")))?;
        let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| LpsError::Lower(format!("{e}")))?;
        drop(naga);
        let module = self
            .engine
            .compile(&ir, &meta)
            .map_err(|e| LpsError::Compile(format!("{e}")))?;
        LpsFragShader::new(module, meta, output_format)
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
