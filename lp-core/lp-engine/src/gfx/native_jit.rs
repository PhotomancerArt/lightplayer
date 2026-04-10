//! RV32 native JIT backend for [`super::LpGraphics`] (`lpvm-native` `rt_jit`).
//!
//! Only built for `riscv32` when feature `native-jit` is enabled (e.g. `fw-emu`).

use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;

use lp_shared::Texture;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine, NativeJitInstance};

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};

/// Graphics backend using in-process RV32 JIT (no Cranelift, no ELF link).
pub struct NativeJitGraphics {
    builtin_table: Arc<BuiltinTable>,
}

impl NativeJitGraphics {
    #[must_use]
    pub fn new() -> Self {
        lps_builtins::ensure_builtins_referenced();
        let mut table = BuiltinTable::new();
        table.populate();
        Self {
            builtin_table: Arc::new(table),
        }
    }
}

impl Default for NativeJitGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for NativeJitGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, crate::error::Error> {
        let naga = lps_frontend::compile(source).map_err(|e| crate::error::Error::Other {
            message: format!("{e}"),
        })?;
        let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| crate::error::Error::Other {
            message: format!("{e}"),
        })?;
        drop(naga);

        let engine = NativeJitEngine::new(
            Arc::clone(&self.builtin_table),
            NativeCompileOptions {
                float_mode: lpir::FloatMode::Q32,
                debug_info: false,
                emu_trace_instructions: false,
                alloc_trace: false,
            },
        );

        let module = engine
            .compile(&ir, &meta)
            .map_err(|e| crate::error::Error::Other {
                message: format!("{e}"),
            })?;

        let has_render = module
            .signatures()
            .functions
            .iter()
            .any(|f| f.name == "render");

        let instance = module
            .instantiate()
            .map_err(|e| crate::error::Error::Other {
                message: format!("{e}"),
            })?;

        let _ = (options.max_errors, options.q32_options);

        Ok(Box::new(NativeJitShader {
            instance,
            has_render,
        }))
    }

    fn backend_name(&self) -> &'static str {
        "native-jit"
    }
}

struct NativeJitShader {
    instance: NativeJitInstance,
    has_render: bool,
}

impl LpShader for NativeJitShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), crate::error::Error> {
        render_native_jit(
            &mut self.instance,
            texture.width(),
            texture.height(),
            time,
            texture,
        )
    }

    fn has_render(&self) -> bool {
        self.has_render
    }
}

fn render_native_jit(
    instance: &mut NativeJitInstance,
    width: u32,
    height: u32,
    time: f32,
    texture: &mut Texture,
) -> Result<(), crate::error::Error> {
    const Q32_SCALE: i32 = 65536;
    let time_q32 = (time * 65536.0 + 0.5) as i32;
    let output_size_q32 = [(width as i32) * Q32_SCALE, (height as i32) * Q32_SCALE];
    for y in 0..height {
        for x in 0..width {
            let frag_coord_q32 = [(x as i32) * Q32_SCALE, (y as i32) * Q32_SCALE];
            let args = [
                frag_coord_q32[0],
                frag_coord_q32[1],
                output_size_q32[0],
                output_size_q32[1],
                time_q32,
            ];
            let rgba_q32 =
                instance
                    .call_q32("render", &args)
                    .map_err(|e| crate::error::Error::Other {
                        message: format!("Shader native JIT call failed: {e}"),
                    })?;

            let clamp_q32 = |v: i32| -> i32 {
                if v < 0 {
                    0
                } else if v > Q32_SCALE {
                    Q32_SCALE
                } else {
                    v
                }
            };

            if rgba_q32.len() < 4 {
                return Err(crate::error::Error::Other {
                    message: format!(
                        "expected 4 return words from render, got {}",
                        rgba_q32.len()
                    ),
                });
            }

            let r = ((clamp_q32(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let g = ((clamp_q32(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let b = ((clamp_q32(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let a = ((clamp_q32(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;

            texture.set_pixel_u16(x, y, [r, g, b, a]);
        }
    }
    Ok(())
}
