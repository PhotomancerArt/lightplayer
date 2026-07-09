//! Authoritative reference frames: the existing host Q32 path
//! (`LpsPxShader::render_frame` on the wasmtime backend), exactly as
//! filetests/fw-checks drive it.

use std::time::{Duration, Instant};

use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, ShaderFrontend};
use lps_shared::{LpsValueF32, TextureBuffer, TextureStorageFormat};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use crate::corpus::CorpusShader;

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

    /// Compile an authored source with the Naga frontend (`lps-frontend`, the
    /// device compile path: `lpfn_*` calls resolve to the Q32 builtin impls)
    /// at the default device Q32 config.
    ///
    /// The corpus forward declarations are spliced ahead of the source for
    /// the same reason as on the GPU path: naga glsl-in resolves calls in
    /// source order. (The alternative `lps-glsl` frontend accepts the
    /// out-of-order source but fails on this backend for `length()` —
    /// `missing import @lpir::sqrt` — so the spike standardizes on Naga.)
    pub fn compile(&self, shader: &CorpusShader) -> Result<ReferenceShader, String> {
        let source = format!("{}{}", shader.forward_decls, shader.source);
        let start = Instant::now();
        let compiled = self
            .engine
            .compile_px_desc(
                CompilePxDesc::new(
                    &source,
                    TextureStorageFormat::Rgba16Unorm,
                    lpir::CompilerConfig::default(),
                )
                .with_frontend(ShaderFrontend::Naga),
            )
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
        let uniforms = LpsValueF32::Struct { name: None, fields };

        compiled
            .shader
            .render_frame(&uniforms, &mut tex)
            .map_err(|e| format!("{}: render_frame: {e:?}", shader.name))?;

        let data = tex.data();
        Ok(data
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::CORPUS;

    #[test]
    fn reference_path_compiles_and_renders_whole_corpus() {
        let renderer = ReferenceRenderer::new().expect("reference renderer");
        for shader in CORPUS {
            let compiled = renderer.compile(shader).expect(shader.name);
            let frame = renderer
                .render(shader, &compiled, 16, 16, 1.0)
                .expect(shader.name);
            assert_eq!(frame.len(), 16 * 16 * 4, "{}", shader.name);
        }
    }
}
