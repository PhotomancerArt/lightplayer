#![no_std]
extern crate alloc;

mod compile;
#[cfg(feature = "cranelift")]
mod render_cranelift;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

#[cfg(feature = "cranelift")]
use lps_shared::lps_value_f32::LpsValueF32;
#[cfg(feature = "cranelift")]
use lpvm::{LpvmInstance, LpvmModule};
#[cfg(feature = "cranelift")]
use lpvm_cranelift::{CompileOptions, CraneliftEngine, FloatMode, MemoryStrategy};

use lpfx::engine::{FxEngine, FxInstance};
use lpfx::texture::{CpuTexture, TextureFormat, TextureId};
use lpfx::{FxModule, FxValue};

#[cfg(feature = "cranelift")]
fn fx_value_to_lps(value: &FxValue) -> LpsValueF32 {
    match value {
        FxValue::F32(v) => LpsValueF32::F32(*v),
        FxValue::I32(v) => LpsValueF32::I32(*v),
        FxValue::Bool(v) => LpsValueF32::Bool(*v),
        FxValue::Vec3(v) => LpsValueF32::Vec3(*v),
    }
}

/// CPU backend: compiles GLSL with Cranelift JIT and renders into [`CpuTexture`] buffers.
pub struct CpuFxEngine {
    textures: BTreeMap<TextureId, CpuTexture>,
    next_id: u32,
}

impl CpuFxEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            textures: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Read-only access to a texture's pixel data.
    #[must_use]
    pub fn texture(&self, id: TextureId) -> Option<&CpuTexture> {
        self.textures.get(&id)
    }

    /// Mutable access to a texture (for writing pixels outside the traits API).
    pub fn texture_mut(&mut self, id: TextureId) -> Option<&mut CpuTexture> {
        self.textures.get_mut(&id)
    }
}

impl Default for CpuFxEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "cranelift")]
pub struct CpuFxInstance {
    input_names: BTreeMap<String, String>,
    output: CpuTexture,
    cranelift: render_cranelift::CraneliftState,
}

#[cfg(feature = "cranelift")]
impl CpuFxInstance {
    #[must_use]
    pub fn output(&self) -> &CpuTexture {
        &self.output
    }
}

#[cfg(feature = "cranelift")]
impl FxInstance for CpuFxInstance {
    type Error = String;

    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error> {
        let uniform_name = self
            .input_names
            .get(name)
            .ok_or_else(|| format!("unknown input: {name}"))?;
        let lps_val = fx_value_to_lps(&value);
        self.cranelift
            .instance
            .set_uniform(uniform_name, &lps_val)
            .map_err(|e| format!("set_uniform: {e}"))?;
        Ok(())
    }

    fn render(&mut self, time: f32) -> Result<(), Self::Error> {
        render_cranelift::render_cranelift(&mut self.cranelift, &mut self.output, time)
    }
}

#[cfg(feature = "cranelift")]
impl FxEngine for CpuFxEngine {
    type Instance = CpuFxInstance;
    type Error = String;

    fn create_texture(&mut self, width: u32, height: u32, format: TextureFormat) -> TextureId {
        let id = TextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.textures
            .insert(id, CpuTexture::new(width, height, format));
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

        let compile = CompileOptions {
            float_mode: FloatMode::Q32,
            memory_strategy: MemoryStrategy::Default,
            max_errors: None,
            emu_trace_instructions: false,
            ..CompileOptions::default()
        };
        let engine = CraneliftEngine::new(compile);

        let compiled = compile::compile_glsl(&engine, &module.glsl_source)?;
        compile::validate_inputs(&module.manifest, &compiled.meta)?;

        let cranelift_mod = compiled.module;
        let instance = cranelift_mod
            .instantiate()
            .map_err(|e| format!("instantiate vm: {e}"))?;
        let direct_call = cranelift_mod
            .direct_call("render")
            .ok_or_else(|| String::from("shader has no `render` entry point"))?;

        let mut input_names = BTreeMap::new();
        for key in module.manifest.inputs.keys() {
            input_names.insert(key.clone(), format!("input_{key}"));
        }

        let state = render_cranelift::CraneliftState {
            _module: cranelift_mod,
            instance,
            direct_call,
        };

        let mut fx = CpuFxInstance {
            input_names,
            output: out_tex,
            cranelift: state,
        };

        for (name, def) in &module.manifest.inputs {
            if let Some(ref val) = def.default {
                FxInstance::set_input(&mut fx, name, val.clone())
                    .map_err(|e| format!("default for `{name}`: {e}"))?;
            }
        }

        Ok(fx)
    }
}

#[cfg(all(test, feature = "cranelift"))]
mod tests {
    use super::*;
    use lpfx::FxModule;

    const NOISE_FX_TOML: &str = include_str!("../../../examples/noise.fx/fx.toml");
    const NOISE_FX_GLSL: &str = include_str!("../../../examples/noise.fx/main.glsl");

    #[test]
    fn noise_fx_renders_nonblack() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(64, 64, TextureFormat::Rgba16);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        instance
            .set_input("speed", FxValue::F32(2.0))
            .expect("set speed");
        instance.render(1.0).expect("render");

        let output = instance.output();
        assert_eq!(output.width(), 64);
        assert_eq!(output.height(), 64);

        let mut nonzero = 0u32;
        for y in 0..64 {
            for x in 0..64 {
                let px = output.pixel_u16(x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    nonzero += 1;
                }
            }
        }
        assert!(
            nonzero > 100,
            "expected many non-black pixels, got {nonzero}"
        );
    }

    #[test]
    fn noise_fx_default_inputs() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(16, 16, TextureFormat::Rgba16);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        instance.render(0.0).expect("render with defaults");

        let output = instance.output();
        let center = output.pixel_u16(8, 8);
        assert!(center[3] > 0, "alpha should be non-zero from render()");
    }
}
