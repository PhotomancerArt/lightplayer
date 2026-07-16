//! Authoritative reference frames: the host Q32 path
//! (`LpsPxShader::render_frame` on the wasmtime backend), exactly as
//! filetests/fw-checks drive it. Ported from `spikes/wgpu-preview-poc`.

use std::time::{Duration, Instant};

use lp_gfx_wgpu::assembly::{authored_prototypes, hoist_declarations};
use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, ShaderFrontend, TextureBuffer};
use lps_shared::{LpsValueF32, TextureStorageFormat};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use super::corpus::CorpusShader;

/// Renders corpus shaders through the Q32 wasm path.
pub struct ReferenceRenderer {
    engine: LpsEngine<WasmLpvmEngine>,
}

/// One compiled reference shader plus its compile duration.
pub struct ReferenceShader {
    shader: LpsPxShader,
    pub compile_time: Duration,
}

impl ReferenceRenderer {
    pub fn new() -> Result<Self, String> {
        let engine =
            WasmLpvmEngine::new(WasmOptions::default()).map_err(|e| format!("wasm engine: {e}"))?;
        Ok(Self {
            engine: LpsEngine::new(engine),
        })
    }

    /// Compile an authored source with the Naga frontend (`lps-frontend`,
    /// the device compile path: `lpfn_*` calls resolve to the Q32 builtin
    /// impls) at the default device Q32 config.
    ///
    /// Struct/const declarations are hoisted and prototypes spliced ahead
    /// of the source for the same reason as on the GPU path: naga glsl-in
    /// resolves calls in source order (the spike hand-declared
    /// `worley_demo` for basic2; here the production hoist + prototype
    /// generator covers every authored declaration).
    pub fn compile(&self, shader: &CorpusShader) -> Result<ReferenceShader, String> {
        let (hoisted, remainder) = hoist_declarations(shader.source);
        let source = format!("{hoisted}{}{remainder}", authored_prototypes(shader.source));
        let start = Instant::now();
        let compiled = self
            .engine
            .compile_px_desc(CompilePxDesc::new(
                &source,
                TextureStorageFormat::Rgba16Unorm,
                lpir::CompilerConfig::default(),
                ShaderFrontend::Naga,
            ))
            .map_err(|e| format!("{}: reference compile: {e:?}", shader.name))?;
        Ok(ReferenceShader {
            shader: compiled,
            compile_time: start.elapsed(),
        })
    }

    /// Render one frame and return tightly packed rgba unorm16 pixels
    /// (row-major, `width * height * 4` values).
    pub fn render(
        &self,
        shader: &CorpusShader,
        compiled: &ReferenceShader,
        width: u32,
        height: u32,
        time: f32,
    ) -> Result<Vec<u16>, String> {
        let mut tex = self
            .engine
            .alloc_texture(width, height, TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| format!("alloc_texture: {e:?}"))?;

        compiled
            .shader
            .render_frame(&corpus_uniforms(shader, width, height, time), &mut tex)
            .map_err(|e| format!("{}: render_frame: {e:?}", shader.name))?;

        let data = tex.data();
        Ok(data
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect())
    }
}

/// The standard engine uniform tree for a corpus shader at (size, time).
pub fn corpus_uniforms(shader: &CorpusShader, width: u32, height: u32, time: f32) -> LpsValueF32 {
    let mut fields: Vec<(String, LpsValueF32)> = vec![
        (
            String::from("outputSize"),
            LpsValueF32::Vec2([width as f32, height as f32]),
        ),
        (String::from("time"), LpsValueF32::F32(time)),
    ];
    for (name, value) in shader.extra_uniforms {
        fields.push((String::from(*name), LpsValueF32::F32(*value)));
    }
    LpsValueF32::Struct { name: None, fields }
}
