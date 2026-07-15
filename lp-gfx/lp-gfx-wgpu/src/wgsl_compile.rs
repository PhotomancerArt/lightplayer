//! GLSL → WGSL translation: assembly → naga `glsl-in` → bounded-tanh pass →
//! validation → `wgsl-out`.
//!
//! naga parse/validation failures surface as [`GfxError::Compile`] carrying
//! the naga diagnostic text (consumed by browser-integration UX later).

use lp_gfx::GfxError;

use crate::assembly::{assemble_fragment_glsl, assemble_sample_fragment_glsl};
use crate::tanh_pass::bound_tanh;

/// A translated fragment shader: WGSL text plus the validated naga module
/// for reflection (uniform layout, P3).
pub struct WgslShader {
    /// The assembled GLSL fed to naga (prelude + prototypes + authored +
    /// wrapper `main`).
    pub assembled_glsl: String,
    /// WGSL text for `wgpu::Device::create_shader_module`.
    pub wgsl: String,
    /// The validated naga module (uniform reflection source of truth).
    pub module: naga::Module,
    /// Validation info for the module.
    pub info: naga::valid::ModuleInfo,
}

/// Translate an authored pixel shader to WGSL at f32 semantics
/// (fullscreen-triangle wrapper: `render(floor(gl_FragCoord.xy))`).
pub fn compile_wgsl(authored: &str) -> Result<WgslShader, GfxError> {
    translate_assembled_glsl(assemble_fragment_glsl(authored))
}

/// Translate the sample-point variant of an authored pixel shader: the same
/// unit with a wrapper `main` that evaluates `render` at a caller-provided
/// position varying (see [`crate::sample_pass`]).
pub fn compile_sample_wgsl(authored: &str) -> Result<WgslShader, GfxError> {
    translate_assembled_glsl(assemble_sample_fragment_glsl(authored))
}

/// naga `glsl-in` → bounded-tanh pass → validation → `wgsl-out` on an
/// already-assembled fragment compilation unit.
fn translate_assembled_glsl(assembled_glsl: String) -> Result<WgslShader, GfxError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let mut module = frontend.parse(&options, &assembled_glsl).map_err(|e| {
        GfxError::Compile(format!(
            "naga glsl-in: {}",
            e.emit_to_string(&assembled_glsl)
        ))
    })?;

    bound_tanh(&mut module).map_err(GfxError::Compile)?;

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    let info = validator.validate(&module).map_err(|e| {
        GfxError::Compile(format!(
            "naga validation: {}",
            e.emit_to_string(&assembled_glsl)
        ))
    })?;

    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| GfxError::Compile(format!("naga wgsl-out: {e}")))?;

    Ok(WgslShader {
        assembled_glsl,
        wgsl,
        module,
        info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_shader_translates_to_wgsl() {
        let shader = compile_wgsl(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
        )
        .expect("translates");
        assert!(shader.wgsl.contains("fn main"), "entry point present");
        assert!(shader.assembled_glsl.contains("void main()"));
    }

    #[test]
    fn sample_unit_translates_with_a_location_zero_input() {
        let shader = compile_sample_wgsl(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
        )
        .expect("translates");
        assert!(shader.wgsl.contains("fn main"), "entry point present");
        assert!(
            shader.wgsl.contains("@location(0)"),
            "sample position varying survives to WGSL:\n{}",
            shader.wgsl
        );
        assert!(shader.assembled_glsl.contains("lp_gfx_sample_pos"));
    }

    #[test]
    fn tanh_is_bounded_in_the_emitted_wgsl() {
        let shader = compile_wgsl(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return tanh(vec4(pos, pos) * 100.0); }\n",
        )
        .expect("translates");
        assert!(
            shader.wgsl.contains("clamp"),
            "bounded tanh:\n{}",
            shader.wgsl
        );
    }

    #[test]
    fn broken_shader_reports_a_compile_error_with_diagnostics() {
        let err = match compile_wgsl("vec4 render(vec2 pos) { return not_defined(pos); }") {
            Err(e) => e,
            Ok(_) => panic!("must not compile"),
        };
        match err {
            GfxError::Compile(message) => {
                assert!(
                    message.contains("naga"),
                    "diagnostic names the stage: {message}"
                );
            }
            other => panic!("expected GfxError::Compile, got {other:?}"),
        }
    }

    #[test]
    fn out_of_order_authored_functions_compile_via_prototypes() {
        let shader = compile_wgsl(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return late(pos); }\n\
             vec4 late(vec2 pos) { return vec4(pos, 0.0, 1.0); }\n",
        )
        .expect("prototype splice closes the declaration-order gap");
        assert!(shader.wgsl.contains("fn main"));
    }
}
