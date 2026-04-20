//! RV32 native JIT backend for [`super::LpGraphics`] (`lpvm-native` `rt_jit`).
//!
//! Compiled when `cfg(target_arch = "riscv32")`. This is the only backend on
//! firmware targets (`fw-emu`, `fw-esp32`).

use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::error::Error;
use crate::gfx::uniforms::build_uniforms;

/// Graphics backend using in-process RV32 JIT (no Cranelift, no ELF link).
pub struct Graphics {
    engine: LpsEngine<NativeJitEngine>,
}

impl Graphics {
    #[must_use]
    pub fn new() -> Self {
        lps_builtins::ensure_builtins_referenced();
        let mut table = BuiltinTable::new();
        table.populate();
        let backend = NativeJitEngine::new(Arc::new(table), NativeCompileOptions::default());
        Self {
            engine: LpsEngine::new(backend),
        }
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for Graphics {
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

        let _ = options.max_errors; // TODO: thread max_errors when front-end accepts it

        Ok(Box::new(NativeJitShader { px }))
    }

    fn backend_name(&self) -> &'static str {
        "lpvm-native::rt_jit"
    }

    fn alloc_output_buffer(&self, width: u32, height: u32) -> Result<LpsTextureBuf, Error> {
        self.engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other {
                message: format!("alloc texture: {e:?}"),
            })
    }
}

struct NativeJitShader {
    px: LpsPxShader,
}

impl LpShader for NativeJitShader {
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
