//! Cranelift JIT backend for [`super::LpGraphics`].

use crate::error::Error;
use crate::gfx::lp_gfx::LpGraphics;
use crate::gfx::lp_shader::{LpShader, ShaderCompileOptions};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use lp_shared::Texture;
use lpvm::{LpvmEngine, VmContextHeader};
use lpvm_cranelift::{
    CompileOptions, CraneliftEngine, CraneliftModule, DirectCall, FloatMode, MemoryStrategy,
};

/// Graphics backend using on-device/host Cranelift JIT.
pub struct CraneliftGraphics;

impl CraneliftGraphics {
    #[must_use]
    pub fn new() -> Self {
        Self
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
        // Frontend: GLSL -> LPIR (using lps_frontend)
        let naga = lps_frontend::compile(source).map_err(|e| Error::Other {
            message: format!("{e}"),
        })?;
        let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| Error::Other {
            message: format!("{e}"),
        })?;
        drop(naga);

        // Backend: LPIR -> machine code (using CraneliftEngine)
        let compile = CompileOptions {
            float_mode: FloatMode::Q32,
            q32_options: options.q32_options,
            memory_strategy: MemoryStrategy::Default,
            max_errors: options.max_errors,
            emu_trace_instructions: false,
            config: lpir::CompilerConfig {
                q32: options.q32_options,
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = CraneliftEngine::new(compile);
        let module = engine.compile(&ir, &meta).map_err(|e| Error::Other {
            message: format!("{e}"),
        })?;
        let direct_call = module.direct_call("render");
        Ok(Box::new(CraneliftShader {
            _module: module,
            direct_call,
        }))
    }

    fn backend_name(&self) -> &'static str {
        "cranelift"
    }
}

struct CraneliftShader {
    _module: CraneliftModule,
    direct_call: Option<DirectCall>,
}

impl LpShader for CraneliftShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error> {
        let dc = self.direct_call.as_ref().ok_or_else(|| Error::Other {
            message: String::from("Shader has no render entry point"),
        })?;
        render_direct_call(dc, texture.width(), texture.height(), time, texture)
    }

    fn has_render(&self) -> bool {
        self.direct_call.is_some()
    }
}

fn render_direct_call(
    dc: &DirectCall,
    width: u32,
    height: u32,
    time: f32,
    texture: &mut Texture,
) -> Result<(), Error> {
    const Q32_SCALE: i32 = 65536;
    let time_q32 = (time * 65536.0 + 0.5) as i32;
    let output_size_q32 = [(width as i32) * Q32_SCALE, (height as i32) * Q32_SCALE];
    let vmctx = VmContextHeader::default();
    let vmctx_ptr = core::ptr::from_ref(&vmctx).cast::<u8>();

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
            let mut rgba_q32 = [0i32; 4];
            unsafe {
                dc.call_i32_buf(vmctx_ptr, &args, &mut rgba_q32)
                    .map_err(|e| Error::Other {
                        message: format!("Shader direct call failed: {e}"),
                    })?;
            }

            let clamp_q32 = |v: i32| -> i32 {
                if v < 0 {
                    0
                } else if v > Q32_SCALE {
                    Q32_SCALE
                } else {
                    v
                }
            };

            let r = ((clamp_q32(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let g = ((clamp_q32(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let b = ((clamp_q32(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let a = ((clamp_q32(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;

            texture.set_pixel_u16(x, y, [r, g, b, a]);
        }
    }
    Ok(())
}
