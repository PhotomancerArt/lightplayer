//! GLSL → WGSL translation via naga (glsl-in → validate → wgsl-out), plus
//! uniform reflection for the minimal uniform mapping.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::corpus::CorpusShader;
use crate::prelude::assemble_prelude;

/// Result of translating one authored shader to WGSL.
pub struct GlslToWgsl {
    /// The assembled GLSL fed to naga (prelude + forward decls + authored +
    /// wrapper `main`).
    pub assembled_glsl: String,
    /// The WGSL text for wgpu.
    pub wgsl: String,
    /// Uniform globals reflected from the naga module (binding order).
    pub uniforms: Vec<UniformSlot>,
    /// Frontend stage timings.
    pub timings: FrontendTimings,
}

/// Per-stage naga timings.
#[derive(Debug, Clone, Copy)]
pub struct FrontendTimings {
    pub parse: Duration,
    pub validate: Duration,
    pub wgsl_out: Duration,
}

/// One reflected uniform: name, `layout(binding = N)` slot, and scalar/vector
/// width in f32 lanes (the corpus only uses `float`/`vec2`; anything else is
/// rejected — noted for M4, where the general `LpsValueF32` tree mapping
/// lives).
#[derive(Debug, Clone, PartialEq)]
pub struct UniformSlot {
    pub name: String,
    pub binding: u32,
    pub lanes: u32,
}

/// A runtime uniform value for [`UniformSlot`].
#[derive(Debug, Clone, Copy)]
pub enum UniformValue {
    F32(f32),
    Vec2([f32; 2]),
}

impl UniformValue {
    /// Raw little-endian bytes as sent to the GPU buffer.
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Self::F32(v) => v.to_le_bytes().to_vec(),
            Self::Vec2(v) => v.iter().flat_map(|c| c.to_le_bytes()).collect(),
        }
    }

    pub fn lanes(&self) -> u32 {
        match self {
            Self::F32(_) => 1,
            Self::Vec2(_) => 2,
        }
    }
}

/// Assemble the full fragment-stage GLSL for a corpus shader: canonical lpfn
/// prelude, forward declarations, the authored source verbatim, and a
/// generated wrapper `main()` matching the CPU path's `pos` convention
/// (integer pixel coordinates; the synthesised `__render_texture_*` loop
/// passes `(x, y)` without a half-pixel offset, so the wrapper floors
/// `gl_FragCoord.xy`).
pub fn assemble_fragment_glsl(shader: &CorpusShader) -> String {
    let prelude = assemble_prelude(shader.source);
    let mut out = String::from("#version 450 core\n");
    out.push_str(&prelude);
    out.push_str(shader.forward_decls);
    out.push_str(shader.source);
    let _ = write!(
        out,
        "\nlayout(location = 0) out vec4 lp_poc_frag_color;\n\
         void main() {{\n    lp_poc_frag_color = render(floor(gl_FragCoord.xy));\n}}\n"
    );
    out
}

/// Translate a corpus shader to WGSL, with validation and reflection.
pub fn glsl_to_wgsl(shader: &CorpusShader) -> Result<GlslToWgsl, String> {
    let assembled_glsl = assemble_fragment_glsl(shader);

    let parse_start = Instant::now();
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let module = frontend
        .parse(&options, &assembled_glsl)
        .map_err(|e| format!("naga glsl-in: {e:?}"))?;
    let parse = parse_start.elapsed();

    let validate_start = Instant::now();
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::default());
    let info = validator
        .validate(&module)
        .map_err(|e| format!("naga validation: {e:?}"))?;
    let validate = validate_start.elapsed();

    let wgsl_out_start = Instant::now();
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| format!("naga wgsl-out: {e}"))?;
    let wgsl_out = wgsl_out_start.elapsed();

    let uniforms = reflect_uniforms(&module)?;

    Ok(GlslToWgsl {
        assembled_glsl,
        wgsl,
        uniforms,
        timings: FrontendTimings {
            parse,
            validate,
            wgsl_out,
        },
    })
}

/// Reflect the module's uniform globals into [`UniformSlot`]s, sorted by
/// binding.
fn reflect_uniforms(module: &naga::Module) -> Result<Vec<UniformSlot>, String> {
    let mut slots = Vec::new();
    for (_, var) in module.global_variables.iter() {
        if var.space != naga::AddressSpace::Uniform {
            continue;
        }
        let name = var
            .name
            .clone()
            .ok_or_else(|| String::from("unnamed uniform global"))?;
        let binding = var
            .binding
            .as_ref()
            .ok_or_else(|| format!("uniform `{name}` has no resource binding"))?;
        if binding.group != 0 {
            return Err(format!(
                "uniform `{name}` uses descriptor set {} (spike supports set 0 only)",
                binding.group
            ));
        }
        let lanes = match &module.types[var.ty].inner {
            naga::TypeInner::Scalar(s) if s.kind == naga::ScalarKind::Float => 1,
            naga::TypeInner::Vector { size, scalar } if scalar.kind == naga::ScalarKind::Float => {
                *size as u32
            }
            other => {
                return Err(format!(
                    "uniform `{name}`: unsupported type {other:?} (spike maps float scalars/vectors only; the general LpsValueF32 mapping is M4 work)"
                ));
            }
        };
        slots.push(UniformSlot {
            name,
            binding: binding.binding,
            lanes,
        });
    }
    slots.sort_by_key(|s| s.binding);
    Ok(slots)
}

/// The standard uniform values for a corpus shader at (size, time).
pub fn uniform_values(
    shader: &CorpusShader,
    slots: &[UniformSlot],
    width: u32,
    height: u32,
    time: f32,
) -> Result<Vec<(UniformSlot, UniformValue)>, String> {
    slots
        .iter()
        .map(|slot| {
            let value = match slot.name.as_str() {
                "outputSize" => UniformValue::Vec2([width as f32, height as f32]),
                "time" => UniformValue::F32(time),
                name => {
                    let (_, v) = shader
                        .extra_uniforms
                        .iter()
                        .find(|(n, _)| *n == name)
                        .ok_or_else(|| format!("no value for uniform `{name}`"))?;
                    UniformValue::F32(*v)
                }
            };
            if value.lanes() != slot.lanes {
                return Err(format!(
                    "uniform `{}`: lane mismatch (shader wants {}, value has {})",
                    slot.name,
                    slot.lanes,
                    value.lanes()
                ));
            }
            Ok((slot.clone(), value))
        })
        .collect()
}

/// Convenience: WGSL shader source cow for wgpu.
pub fn wgsl_source(wgsl: &str) -> wgpu::ShaderSource<'_> {
    wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::CORPUS;

    #[test]
    fn all_corpus_shaders_translate_to_wgsl() {
        for shader in CORPUS {
            let result = glsl_to_wgsl(shader);
            let translated = match result {
                Ok(t) => t,
                Err(e) => panic!("{}: {e}", shader.name),
            };
            assert!(
                translated.wgsl.contains("fn main"),
                "{}: WGSL should contain the fragment entry point",
                shader.name
            );
        }
    }

    #[test]
    fn corpus_uniforms_reflect_output_size_and_time() {
        for shader in CORPUS {
            let translated = glsl_to_wgsl(shader).expect(shader.name);
            let names: Vec<&str> = translated
                .uniforms
                .iter()
                .map(|u| u.name.as_str())
                .collect();
            assert!(names.contains(&"outputSize"), "{}", shader.name);
            assert!(names.contains(&"time"), "{}", shader.name);
            uniform_values(shader, &translated.uniforms, 128, 128, 1.0)
                .unwrap_or_else(|e| panic!("{}: {e}", shader.name));
        }
    }
}
