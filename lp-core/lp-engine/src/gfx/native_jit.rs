//! RV32 native JIT backend for [`super::LpGraphics`] (`lpvm-native-fa` `rt_jit`).
//!
//! Only built for `riscv32` when feature `native-jit` is enabled (e.g. `fw-emu`).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use lp_shared::Texture;
use lpvm::{LpvmEngine, LpvmModule};
use lpvm_native_fa::{
    BuiltinTable, NativeCompileOptions, NativeJitDirectCall, NativeJitEngine, NativeJitInstance,
};

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
        log::debug!("[native-jit] Starting GLSL compilation");
        let naga = lps_frontend::compile(source).map_err(|e| crate::error::Error::Other {
            message: format!("{e}"),
        })?;
        log::debug!("[native-jit] Naga parsing complete, lowering to LPIR...");
        let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| crate::error::Error::Other {
            message: format!("{e}"),
        })?;
        log::debug!("[native-jit] LPIR lowering complete: {} functions", ir.functions.len());
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

        log::debug!("[native-jit] Compiling LPIR to native code...");
        let module = engine
            .compile(&ir, &meta)
            .map_err(|e| crate::error::Error::Other {
                message: format!("{e}"),
            })?;
        log::debug!("[native-jit] Native compilation complete");

        let direct_call = module.direct_call("render");

        let instance = module
            .instantiate()
            .map_err(|e| crate::error::Error::Other {
                message: format!("{e}"),
            })?;

        let _ = (options.max_errors, options.q32_options);

        Ok(Box::new(NativeJitShader {
            instance,
            direct_call,
        }))
    }

    fn backend_name(&self) -> &'static str {
        "native-jit"
    }
}

struct NativeJitShader {
    instance: NativeJitInstance,
    direct_call: Option<NativeJitDirectCall>,
}

impl LpShader for NativeJitShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), crate::error::Error> {
        let dc = self
            .direct_call
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other {
                message: String::from("Shader has no render entry point"),
            })?;
        render_native_jit_direct(
            &mut self.instance,
            dc,
            texture.width(),
            texture.height(),
            time,
            texture,
        )
    }

    fn has_render(&self) -> bool {
        self.direct_call.is_some()
    }
}

fn render_native_jit_direct(
    instance: &mut NativeJitInstance,
    dc: &NativeJitDirectCall,
    width: u32,
    height: u32,
    time: f32,
    texture: &mut Texture,
) -> Result<(), crate::error::Error> {
    const Q32_SCALE: i32 = 65536;
    let time_q32 = (time * 65536.0 + 0.5) as i32;
    let output_size_q32 = [(width as i32) * Q32_SCALE, (height as i32) * Q32_SCALE];

    let clamp_q32 = |v: i32| -> i32 {
        if v < 0 {
            0
        } else if v > Q32_SCALE {
            Q32_SCALE
        } else {
            v
        }
    };

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

            // Stack-allocated return buffer (no heap allocation!)
            let mut rgba_q32 = [0i32; 4];
            instance
                .call_direct(dc, &args, &mut rgba_q32)
                .map_err(|e| crate::error::Error::Other {
                    message: format!("Shader native JIT call failed: {e}"),
                })?;

            let r = ((clamp_q32(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let g = ((clamp_q32(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let b = ((clamp_q32(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let a = ((clamp_q32(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;

            texture.set_pixel_u16(x, y, [r, g, b, a]);
        }
    }
    Ok(())
}
