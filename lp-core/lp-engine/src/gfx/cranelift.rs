//! Cranelift JIT backend for [`super::LpGraphics`].

use crate::error::Error;
use crate::gfx::lp_gfx::LpGraphics;
use crate::gfx::lp_shader::{LpShader, ShaderCompileOptions};
use crate::gfx::uniforms::build_uniforms;
use alloc::boxed::Box;
use alloc::format;
use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_cranelift::{CompileOptions, CraneliftEngine};

/// Graphics backend using on-device/host Cranelift JIT.
pub struct CraneliftGraphics {
    engine: LpsEngine<CraneliftEngine>,
}

impl CraneliftGraphics {
    #[must_use]
    pub fn new() -> Self {
        let backend = CraneliftEngine::new(CompileOptions::default());
        Self {
            engine: LpsEngine::new(backend),
        }
    }
}

impl Default for CraneliftGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let cfg = options.to_compiler_config();
        let px = self
            .engine
            .compile_px(source, lps_shared::TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(CraneliftShader { px }))
    }

    fn backend_name(&self) -> &'static str {
        "cranelift"
    }

    fn alloc_output_buffer(&self, width: u32, height: u32) -> Result<LpsTextureBuf, Error> {
        self.engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other {
                message: format!("alloc texture: {e:?}"),
            })
    }
}

struct CraneliftShader {
    px: LpsPxShader,
}

impl LpShader for CraneliftShader {
    fn render(&mut self, buf: &mut LpsTextureBuf, time: f32) -> Result<(), Error> {
        let uniforms = build_uniforms(buf.width(), buf.height(), time);
        self.px
            .render_frame(&uniforms, buf)
            .map_err(|e| Error::Other {
                message: format!("render_frame: {e}"),
            })
    }

    fn has_render(&self) -> bool {
        true
    }
}
