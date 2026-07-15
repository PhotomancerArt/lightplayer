//! GLSL → WGSL translation: assembly → naga `glsl-in` → bounded-tanh pass →
//! validation → `wgsl-out`.
//!
//! naga parse/validation failures surface as [`GfxError::Compile`] carrying
//! the naga diagnostic text (consumed by browser-integration UX later).

use lp_gfx::GfxError;
use lp_shader::TextureBindingSpecs;

use crate::assembly::assemble_fragment_glsl;
use crate::tanh_pass::bound_tanh;
use crate::uniform_layout::assign_texture_bindings;

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

/// Translate an authored pixel shader to WGSL at f32 semantics.
///
/// `textures` is the compile-time `TextureBindingSpec` map; sampling call
/// sites are lowered against it during assembly and the resulting texture
/// globals get `@group(0)` bindings assigned before validation.
pub fn compile_wgsl(
    authored: &str,
    textures: &TextureBindingSpecs,
) -> Result<WgslShader, GfxError> {
    let assembled_glsl = assemble_fragment_glsl(authored, textures)?;

    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let mut module = frontend.parse(&options, &assembled_glsl).map_err(|e| {
        GfxError::Compile(format!(
            "naga glsl-in: {}",
            e.emit_to_string(&assembled_glsl)
        ))
    })?;

    assign_texture_bindings(&mut module)?;
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
    use lp_shader::texture_binding;
    use lps_shared::{TextureFilter, TextureStorageFormat, TextureWrap};

    fn compile_wgsl_no_textures(authored: &str) -> Result<WgslShader, GfxError> {
        compile_wgsl(authored, &TextureBindingSpecs::new())
    }

    #[test]
    fn minimal_shader_translates_to_wgsl() {
        let shader = compile_wgsl_no_textures(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
        )
        .expect("translates");
        assert!(shader.wgsl.contains("fn main"), "entry point present");
        assert!(shader.assembled_glsl.contains("void main()"));
    }

    #[test]
    fn tanh_is_bounded_in_the_emitted_wgsl() {
        let shader = compile_wgsl_no_textures(
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
        let err =
            match compile_wgsl_no_textures("vec4 render(vec2 pos) { return not_defined(pos); }") {
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
        let shader = compile_wgsl_no_textures(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return late(pos); }\n\
             vec4 late(vec2 pos) { return vec4(pos, 0.0, 1.0); }\n",
        )
        .expect("prototype splice closes the declaration-order gap");
        assert!(shader.wgsl.contains("fn main"));
    }

    #[test]
    fn sampler_uniform_translates_to_a_bound_texture_load() {
        let mut textures = TextureBindingSpecs::new();
        textures.insert(
            String::from("inputColor"),
            texture_binding::texture2d(
                TextureStorageFormat::Rgba16Unorm,
                TextureFilter::Nearest,
                TextureWrap::ClampToEdge,
                TextureWrap::ClampToEdge,
            ),
        );
        let shader = compile_wgsl(
            "uniform sampler2D inputColor;\n\
             vec4 render(vec2 pos) { return texelFetch(inputColor, ivec2(pos), 0); }\n",
            &textures,
        )
        .expect("translates");
        assert!(
            shader.wgsl.contains("textureLoad"),
            "fetch lowers to textureLoad:\n{}",
            shader.wgsl
        );
        assert!(
            shader.wgsl.contains("@group(0)"),
            "texture global is bound:\n{}",
            shader.wgsl
        );
        assert!(
            !shader.wgsl.contains("textureSample"),
            "no hardware sampler path:\n{}",
            shader.wgsl
        );
    }

    #[test]
    fn filtered_sampling_translates_without_sampler_bindings() {
        let mut textures = TextureBindingSpecs::new();
        textures.insert(
            String::from("t"),
            texture_binding::texture2d(
                TextureStorageFormat::Rgba16Unorm,
                TextureFilter::Linear,
                TextureWrap::Repeat,
                TextureWrap::MirrorRepeat,
            ),
        );
        let shader = compile_wgsl(
            "uniform sampler2D t;\n\
             vec4 render(vec2 pos) { return texture(t, pos / 8.0); }\n",
            &textures,
        )
        .expect("translates");
        assert!(shader.wgsl.contains("textureLoad"), "{}", shader.wgsl);
        assert!(
            !shader.wgsl.contains(": sampler"),
            "manual bilinear needs no sampler global:\n{}",
            shader.wgsl
        );
    }
}
