#![no_std]
//! CPU backend for [`lpfx`] modules: compiles GLSL via `lp-shader`
//! and renders into [`LpsTextureBuf`] outputs.
//!
//! Backend selection is target-driven (see [`backend`]). One
//! [`CpuFxEngine`] owns one [`LpsEngine`], which owns one
//! [`lpvm::LpvmEngine`] — engines are 1-to-1-to-1.

extern crate alloc;

mod backend;
mod compile;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lpir::CompilerConfig;
use lps_shared::lps_value_f32::LpsValueF32;
use lps_shared::{TextureBuffer, TextureStorageFormat};

use lpfx::engine::{FxEngine, FxInstance};
use lpfx::texture::TextureId;
use lpfx::{FxModule, FxRenderInputs, FxValue};

use crate::backend::{LpvmBackend, new_backend};

/// CPU effect engine: one shared LPVM backend, one shared
/// `LpsEngine`, one bump-allocated texture pool.
///
/// Every [`Self::create_texture`] and [`Self::instantiate`] call
/// reuses the same underlying `LpsEngine`, so all textures and
/// compiled shaders share a single LPVM memory pool. The pool is a
/// bump allocator on host (M4b pre-grows wasmtime linear memory to
/// 64 MiB ≈ 8M `Rgba16Unorm` pixels); textures are not freed
/// individually — only dropping the whole `CpuFxEngine` reclaims
/// the pool.
pub struct CpuFxEngine {
    engine: LpsEngine<LpvmBackend>,
    textures: BTreeMap<TextureId, LpsTextureBuf>,
    next_id: u32,
}

impl CpuFxEngine {
    /// New engine with the target-arch-default LPVM backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: LpsEngine::new(new_backend()),
            textures: BTreeMap::new(),
            next_id: 0,
        }
    }
}

impl Default for CpuFxEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// One running effect instance.
///
/// Holds a compiled [`LpsPxShader`] and the [`LpsTextureBuf`] it
/// renders into. Inputs are supplied per-render via
/// [`FxRenderInputs`]; nothing is cached on the instance between
/// renders.
pub struct CpuFxInstance {
    /// Manifest input name → shader uniform name (`speed` → `input_speed`).
    input_names: BTreeMap<String, String>,
    output: LpsTextureBuf,
    px: LpsPxShader,
}

impl CpuFxInstance {
    /// Read-only access to the output buffer (use [`TextureBuffer::data`]
    /// for raw bytes).
    #[must_use]
    pub fn output(&self) -> &LpsTextureBuf {
        &self.output
    }
}

impl FxEngine for CpuFxEngine {
    type Instance = CpuFxInstance;
    type Error = String;

    fn create_texture(&mut self, width: u32, height: u32) -> TextureId {
        let id = TextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let buf = self
            .engine
            .alloc_texture(width, height, TextureStorageFormat::Rgba16Unorm)
            .expect("alloc Rgba16Unorm texture from shared LPVM memory");
        self.textures.insert(id, buf);
        id
    }

    fn instantiate(
        &mut self,
        module: &FxModule,
        output: TextureId,
    ) -> Result<Self::Instance, Self::Error> {
        let out_tex = self
            .textures
            .remove(&output)
            .ok_or_else(|| format!("unknown texture id {}", output.raw()))?;

        let cfg = CompilerConfig::default();
        let px = self
            .engine
            .compile_px(&module.glsl_source, TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| format!("compile_px: {e}"))?;

        compile::validate_inputs(&module.manifest, px.meta())?;

        let mut input_names = BTreeMap::new();
        for key in module.manifest.inputs.keys() {
            input_names.insert(key.clone(), format!("input_{key}"));
        }

        Ok(CpuFxInstance {
            input_names,
            output: out_tex,
            px,
        })
    }
}

impl FxInstance for CpuFxInstance {
    type Error = String;

    fn render(&mut self, inputs: &FxRenderInputs<'_>) -> Result<(), Self::Error> {
        let width = self.output.width();
        let height = self.output.height();

        let mut fields: Vec<(String, LpsValueF32)> = Vec::with_capacity(2 + inputs.inputs.len());
        fields.push((
            String::from("outputSize"),
            LpsValueF32::Vec2([width as f32, height as f32]),
        ));
        fields.push((String::from("time"), LpsValueF32::F32(inputs.time)));

        for (name, value) in inputs.inputs {
            let uniform_name = self
                .input_names
                .get(*name)
                .ok_or_else(|| format!("unknown input: {name}"))?;
            fields.push((uniform_name.clone(), fx_value_to_lps(value)));
        }

        let uniforms = LpsValueF32::Struct { name: None, fields };

        self.px
            .render_frame(&uniforms, &mut self.output)
            .map_err(|e| format!("render_frame: {e}"))
    }
}

fn fx_value_to_lps(value: &FxValue) -> LpsValueF32 {
    match value {
        FxValue::F32(v) => LpsValueF32::F32(*v),
        FxValue::I32(v) => LpsValueF32::I32(*v),
        FxValue::Bool(v) => LpsValueF32::Bool(*v),
        FxValue::Vec3(v) => LpsValueF32::Vec3(*v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfx::{FxModule, defaults_from_manifest};

    const NOISE_FX_TOML: &str = include_str!("../../../examples/noise.fx/fx.toml");
    const NOISE_FX_GLSL: &str = include_str!("../../../examples/noise.fx/main.glsl");

    /// Read pixel `(x, y)` of an Rgba16Unorm texture as four `u16`s
    /// (little-endian). Test-only readback helper.
    fn pixel_u16(buf: &LpsTextureBuf, x: u32, y: u32) -> [u16; 4] {
        let bpp = buf.format().bytes_per_pixel();
        let offset = ((y as usize) * (buf.width() as usize) + x as usize) * bpp;
        let bytes = &buf.data()[offset..offset + bpp];
        [
            u16::from_le_bytes([bytes[0], bytes[1]]),
            u16::from_le_bytes([bytes[2], bytes[3]]),
            u16::from_le_bytes([bytes[4], bytes[5]]),
            u16::from_le_bytes([bytes[6], bytes[7]]),
        ]
    }

    #[test]
    fn noise_fx_renders_nonblack() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        // Correctness test: tiny is fine. Realistic resolutions belong in
        // a perf suite, not here.
        const SZ: u32 = 4;
        let tex = engine.create_texture(SZ, SZ);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        // Seed defaults, then overlay the user-driven `speed`.
        let mut defaults = defaults_from_manifest(&module.manifest);
        for (name, value) in defaults.iter_mut() {
            if name == "speed" {
                *value = FxValue::F32(2.0);
            }
        }
        let inputs: alloc::vec::Vec<(&str, FxValue)> = defaults
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect();
        let render_inputs = FxRenderInputs {
            time: 1.0,
            inputs: &inputs,
        };

        instance.render(&render_inputs).expect("render");

        let output = instance.output();
        assert_eq!(output.width(), SZ);
        assert_eq!(output.height(), SZ);

        let mut nonzero = 0u32;
        for y in 0..SZ {
            for x in 0..SZ {
                let px = pixel_u16(output, x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    nonzero += 1;
                }
            }
        }
        assert!(nonzero > 0, "expected at least one non-black pixel");
    }

    #[test]
    fn noise_fx_default_inputs() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        const SZ: u32 = 4;
        let tex = engine.create_texture(SZ, SZ);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        let defaults = defaults_from_manifest(&module.manifest);
        let inputs: alloc::vec::Vec<(&str, FxValue)> = defaults
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect();
        let render_inputs = FxRenderInputs {
            time: 0.0,
            inputs: &inputs,
        };
        instance
            .render(&render_inputs)
            .expect("render with defaults");

        let center = pixel_u16(instance.output(), SZ / 2, SZ / 2);
        assert!(center[3] > 0, "alpha should be non-zero from render()");
    }
}
