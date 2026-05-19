//! Host graphics backend (`lpvm-wasm` `rt_wasmtime`).
//!
//! Compiled on every target except `riscv32` and `wasm32`. Wraps
//! [`lpvm_wasm::rt_wasmtime::WasmLpvmEngine`], so all of LPIR → WASM →
//! wasmtime JIT happens in-process. Pre-grows linear memory once
//! per engine (see [`lpvm_wasm::WasmOptions::host_memory_pages`]) so
//! cached `LpvmBuffer` host pointers stay valid.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, LpsTextureBuf};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpComputeShader, LpShader, ShaderCompileOptions, ShaderCompileStats};
use crate::engine::error::Error;

/// Host shader graphics backed by `lpvm-wasm` + wasmtime.
pub struct Graphics {
    engine: LpsEngine<WasmLpvmEngine>,
}

impl Graphics {
    /// New host graphics with default `WasmOptions`.
    pub fn new() -> Self {
        let backend = WasmLpvmEngine::new(WasmOptions::default())
            .expect("WasmLpvmEngine::new with default WasmOptions");
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
            .compile_px_desc(
                CompilePxDesc::new(source, lps_shared::TextureStorageFormat::Rgba16Unorm, cfg)
                    .with_frontend(options.frontend),
            )
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(HostShader { px }))
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, Error> {
        let shader = self
            .engine
            .compile_compute_desc(desc)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(shader))
    }

    fn backend_name(&self) -> &'static str {
        "lpvm-wasm::rt_wasmtime"
    }

    fn alloc_output_buffer(&self, width: u32, height: u32) -> Result<LpsTextureBuf, Error> {
        self.engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other {
                message: format!("alloc texture: {e:?}"),
            })
    }

    fn free_output_buffer(&self, buffer: LpsTextureBuf) {
        self.engine.free_texture(buffer);
    }

    fn alloc_sample_points(&self, count: u32) -> Result<lp_shader::LpsSamplePointBuf, Error> {
        self.engine
            .alloc_sample_points(count)
            .map_err(|e| Error::Other {
                message: format!("alloc sample points: {e:?}"),
            })
    }

    fn alloc_sample_rgba16(&self, count: u32) -> Result<lp_shader::LpsSampleRgba16Buf, Error> {
        self.engine
            .alloc_sample_rgba16(count)
            .map_err(|e| Error::Other {
                message: format!("alloc sample rgba16: {e:?}"),
            })
    }

    fn free_sample_points(&self, buffer: lp_shader::LpsSamplePointBuf) {
        self.engine.free_sample_points(buffer);
    }

    fn free_sample_rgba16(&self, buffer: lp_shader::LpsSampleRgba16Buf) {
        self.engine.free_sample_rgba16(buffer);
    }
}

struct HostShader {
    px: LpsPxShader,
}

impl LpShader for HostShader {
    fn render(
        &mut self,
        buf: &mut LpsTextureBuf,
        uniforms: &lps_shared::LpsValueF32,
    ) -> Result<(), Error> {
        self.px
            .render_frame(uniforms, buf)
            .map_err(|e| Error::Other {
                message: format!("render_frame: {e}"),
            })
    }

    fn sample_rgba16(
        &mut self,
        points: &mut lp_shader::LpsSamplePointBuf,
        out: &mut lp_shader::LpsSampleRgba16Buf,
        uniforms: &lps_shared::LpsValueF32,
    ) -> Result<(), Error> {
        self.px
            .sample_points_rgba16(uniforms, points, out)
            .map_err(|e| Error::Other {
                message: format!("sample_points_rgba16: {e}"),
            })
    }

    fn has_render(&self) -> bool {
        true
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        Some(self.px.compile_stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::uniforms::build_uniforms;
    use crate::gfx::{LpGraphics, ShaderCompileOptions};

    #[test]
    fn direct_sampling_uses_requested_output_size_uniform() {
        let graphics = Graphics::new();
        let mut shader = graphics
            .compile_shader(
                "layout(binding = 0) uniform vec2 outputSize;\n\
                 vec4 render(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
                &ShaderCompileOptions::default(),
            )
            .expect("compile shader");

        let mut points = graphics.alloc_sample_points(2).expect("points");
        points
            .data_mut()
            .copy_from_slice(&[0, 0, 2 * 65536, 4 * 65536]);
        let mut out = graphics.alloc_sample_rgba16(2).expect("out");
        let uniforms = build_uniforms(4, 8, &[]);

        shader
            .sample_rgba16(&mut points, &mut out, &uniforms)
            .expect("sample");

        assert_eq!(&out.data()[0..4], &[0, 0, 0, 65535]);
        assert_eq!(&out.data()[4..8], &[32768, 32768, 0, 65535]);
    }
}
