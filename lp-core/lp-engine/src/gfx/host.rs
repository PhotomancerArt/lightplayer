//! Host graphics backend (`lpvm-wasm` `rt_wasmtime`).
//!
//! Compiled on every target except `riscv32` and `wasm32`. Wraps
//! [`lpvm_wasm::rt_wasmtime::WasmLpvmEngine`], so all of LPIR → WASM →
//! wasmtime JIT happens in-process. Pre-grows linear memory once
//! per engine (see [`lpvm_wasm::WasmOptions::host_memory_pages`]) so
//! cached `LpvmBuffer` host pointers stay valid.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::error::Error;
use crate::gfx::uniforms::build_uniforms;

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
            .compile_px(source, lps_shared::TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(HostShader { px }))
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
}

struct HostShader {
    px: LpsPxShader,
}

impl LpShader for HostShader {
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
