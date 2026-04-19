//! Wasm32 guest graphics backend (`lpvm-wasm` `rt_browser`).
//!
//! Compiled when `cfg(target_arch = "wasm32")`. Wraps
//! [`lpvm_wasm::rt_browser::BrowserLpvmEngine`] which runs the
//! emitted shader WASM via the host JS `WebAssembly.Module` /
//! `Instance` API.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_browser::BrowserLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::error::Error;
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
            .compile_px(source, lps_shared::TextureStorageFormat::Rgba16Unorm, &cfg)
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

    fn has_render(&self) -> bool {
        true
    }
}
