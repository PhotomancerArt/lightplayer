//! Wasm32 guest graphics backend (`lpvm-wasm` `rt_browser`).
//!
//! Compiled when `cfg(target_arch = "wasm32")`. Wraps
//! [`lpvm_wasm::rt_browser::BrowserLpvmEngine`] which runs the
//! emitted shader WASM via the host JS `WebAssembly.Module` /
//! `Instance` API.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, LpsTextureBuf};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_browser::BrowserLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::engine::error::Error;
use crate::gfx::uniforms::build_uniforms;

/// Wasm32 guest shader graphics backed by `lpvm-wasm` + browser host.
pub struct Graphics {
    engine: LpsEngine<BrowserLpvmEngine>,
}

impl Graphics {
    /// New guest graphics with default `WasmOptions`.
    pub fn new() -> Self {
        let backend = BrowserLpvmEngine::new(WasmOptions::default())
            .expect("BrowserLpvmEngine::new with default WasmOptions");
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
        Ok(Box::new(WasmGuestShader { px }))
    }

    fn backend_name(&self) -> &'static str {
        "lpvm-wasm::rt_browser"
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

struct WasmGuestShader {
    px: LpsPxShader,
}

impl LpShader for WasmGuestShader {
    fn render(&mut self, buf: &mut LpsTextureBuf, time: f32) -> Result<(), Error> {
        let uniforms = build_uniforms(buf.width(), buf.height(), time);
        self.px
            .render_frame(&uniforms, buf)
            .map_err(|e| Error::Other {
                message: format!("render_frame: {e}"),
            })
    }

    fn sample_rgba16(
        &mut self,
        points: &mut lp_shader::LpsSamplePointBuf,
        out: &mut lp_shader::LpsSampleRgba16Buf,
        output_width: u32,
        output_height: u32,
        time: f32,
    ) -> Result<(), Error> {
        let uniforms = build_uniforms(output_width, output_height, time);
        self.px
            .sample_points_rgba16(&uniforms, points, out)
            .map_err(|e| Error::Other {
                message: format!("sample_points_rgba16: {e}"),
            })
    }

    fn has_render(&self) -> bool {
        true
    }
}
