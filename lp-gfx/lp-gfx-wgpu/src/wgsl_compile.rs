//! GLSL → WGSL translation: assembly → naga `glsl-in` → bounded-tanh pass →
//! loop bounds → validation → `wgsl-out`.
//!
//! naga parse/validation failures and the loop refusal surface as
//! [`GfxError::Compile`] carrying a codespan-style diagnostic whose
//! `┌─ glsl:LINE:COL` marker is in **authored** coordinates: spans are
//! shifted out of the assembled unit ([`AssembledGlsl::to_authored`]) before
//! rendering, so the Studio editor (`ui_shader_error.rs`) highlights the line
//! the author wrote, exactly as it does for the LPVM tiers (whose naga
//! frontend remaps over its own prefix in `lps-frontend/src/parse.rs`). A
//! diagnostic whose span lies outside the authored text — the lpfn prelude,
//! a hoisted declaration's copy, a texture helper, the wrapper `main` —
//! renders as a message with no location rather than a misleading line.

use lp_gfx::GfxError;
use lp_shader::{DEFAULT_INVOCATION_FUEL, ShaderEntrySpace, TextureBindingSpecs};

use crate::assembly::{AssembledGlsl, assemble_fragment_glsl, assemble_sample_fragment_glsl};
use crate::loop_bound_pass::{bound_loop_iterations, refuse_loops_without_exit};
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

/// Translate an authored pixel shader to WGSL at f32 semantics
/// (fullscreen-triangle wrapper around the declared entry, e.g.
/// `render_2d(floor(gl_FragCoord.xy))`).
///
/// `textures` is the compile-time `TextureBindingSpec` map; sampling call
/// sites are lowered against it during assembly and the resulting texture
/// globals get `@group(0)` bindings assigned before validation.
pub fn compile_wgsl(
    authored: &str,
    textures: &TextureBindingSpecs,
    space: ShaderEntrySpace,
) -> Result<WgslShader, GfxError> {
    translate_assembled_glsl(assemble_fragment_glsl(authored, textures, space)?)
}

/// Translate the sample-point variant of an authored pixel shader: the same
/// unit with a wrapper `main` that evaluates the declared entry at a
/// caller-provided position varying (see [`crate::sample_pass`]).
pub fn compile_sample_wgsl(
    authored: &str,
    textures: &TextureBindingSpecs,
    space: ShaderEntrySpace,
) -> Result<WgslShader, GfxError> {
    translate_assembled_glsl(assemble_sample_fragment_glsl(authored, textures, space)?)
}

/// naga `glsl-in` → bounded-tanh pass → loop bounds → validation →
/// `wgsl-out` on an already-assembled fragment compilation unit.
fn translate_assembled_glsl(assembled: AssembledGlsl) -> Result<WgslShader, GfxError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let mut module = frontend.parse(&options, &assembled.glsl).map_err(|e| {
        GfxError::Compile(format!(
            "naga glsl-in: {}",
            render_parse_errors_at_authored_lines(&e, &assembled)
        ))
    })?;

    assign_texture_bindings(&mut module)?;
    bound_tanh(&mut module).map_err(GfxError::Compile)?;
    // The GPU tier's loop contract (`loop_bound_pass`): a loop that can never
    // exit is a compile error, and every loop that remains charges the same
    // per-invocation back-edge budget the LPVM tiers meter as fuel.
    refuse_loops_without_exit(&module, &assembled).map_err(GfxError::Compile)?;
    bound_loop_iterations(&mut module, DEFAULT_INVOCATION_FUEL);

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    let info = validator.validate(&module).map_err(|e| {
        GfxError::Compile(format!(
            "naga validation: {}",
            render_validation_error_at_authored_lines(e, &assembled)
        ))
    })?;

    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| GfxError::Compile(format!("naga wgsl-out: {e}")))?;

    Ok(WgslShader {
        assembled_glsl: assembled.glsl,
        wgsl,
        module,
        info,
    })
}

/// Render naga parse errors with their `┌─ glsl:LINE:COL` marker in authored
/// coordinates: every span is shifted out of the assembled unit and the
/// diagnostic is drawn against the authored slice, so the gutter shows the
/// authored line too. A span outside the authored text becomes undefined,
/// which naga renders as the message alone.
fn render_parse_errors_at_authored_lines(
    errors: &naga::front::glsl::ParseErrors,
    unit: &AssembledGlsl,
) -> String {
    let remapped: Vec<naga::front::glsl::Error> = errors
        .errors
        .iter()
        .map(|e| naga::front::glsl::Error {
            kind: e.kind.clone(),
            meta: unit.to_authored(e.meta),
        })
        .collect();
    naga::front::glsl::ParseErrors::from(remapped)
        .emit_to_string_with_path(unit.authored_text(), "glsl")
}

/// The same remap for a validation error. naga labels these `wgsl` by
/// default, a marker the Studio parser does not know; the unit is GLSL, so
/// the label says so and the editor locates validation errors as well.
fn render_validation_error_at_authored_lines(
    error: naga::WithSpan<naga::valid::ValidationError>,
    unit: &AssembledGlsl,
) -> String {
    let spans: Vec<(naga::Span, String)> = error
        .spans()
        .map(|(span, description)| (unit.to_authored(*span), description.clone()))
        .collect();
    // `with_span` drops an undefined span, so out-of-region labels vanish
    // instead of pointing at line 1.
    let mut remapped = naga::WithSpan::new(error.into_inner());
    for (span, description) in spans {
        remapped = remapped.with_span(span, description);
    }
    remapped.emit_to_string_with_path(unit.authored_text(), "glsl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_bound_pass::LOOP_BUDGET_GLOBAL;
    use lp_shader::texture_binding;
    use lps_shared::{TextureFilter, TextureStorageFormat, TextureWrap};

    fn compile_wgsl_no_textures(authored: &str) -> Result<WgslShader, GfxError> {
        compile_wgsl(
            authored,
            &TextureBindingSpecs::new(),
            ShaderEntrySpace::TwoD,
        )
    }

    /// The fault-demo rig's authored shader: `projects/test/fault-demo`
    /// (its home once the catalog tree lands, PR #536) or `examples/fault-demo`
    /// (main's, until then). Read at test time so the pin follows the file.
    fn fault_demo_source() -> String {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let candidates = [
            "projects/test/fault-demo/effect/shader.glsl",
            "examples/fault-demo/shader.glsl",
        ];
        candidates
            .iter()
            .find_map(|candidate| std::fs::read_to_string(format!("{root}/{candidate}")).ok())
            .unwrap_or_else(|| panic!("fault-demo shader not found at any of {candidates:?}"))
    }

    #[test]
    fn minimal_shader_translates_to_wgsl() {
        let shader = compile_wgsl_no_textures(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
        )
        .expect("translates");
        assert!(shader.wgsl.contains("fn main"), "entry point present");
        assert!(shader.assembled_glsl.contains("void main()"));
    }

    #[test]
    fn sample_unit_translates_with_a_location_zero_input() {
        let shader = compile_sample_wgsl(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
            &TextureBindingSpecs::new(),
            ShaderEntrySpace::TwoD,
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
        let shader = compile_wgsl_no_textures(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) { return tanh(vec4(pos, pos) * 100.0); }\n",
        )
        .expect("translates");
        assert!(
            shader.wgsl.contains("clamp"),
            "bounded tanh:\n{}",
            shader.wgsl
        );
    }

    /// The defect's subject (`docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`):
    /// the fault-demo rig's `while (true)` is refused before anything reaches
    /// a device. Nothing here creates a wgpu device — the assertion is on the
    /// compile refusal alone, which is the only safe way to test it.
    #[test]
    fn fault_demo_is_refused_at_gpu_compile_time() {
        let source = fault_demo_source();
        assert!(
            source.contains("while (true)"),
            "the rig still carries its unbounded loop:\n{source}"
        );
        let err = compile_wgsl_no_textures(&source)
            .err()
            .expect("fault-demo must not compile on the GPU tier");
        match err {
            GfxError::Compile(message) => {
                assert!(message.contains("unbounded loop"), "{message}");
                assert!(
                    message.contains("`render_2d`"),
                    "names the function: {message}"
                );
                assert!(
                    message.contains("while (true)"),
                    "quotes the loop head: {message}"
                );
                // The line is the AUTHORED one (read from the rig, not
                // hard-coded), carried both in the headline and in the
                // codespan-style marker the Studio parser locates by.
                let authored_line = source
                    .lines()
                    .position(|line| line.contains("while (true)"))
                    .expect("loop head line")
                    + 1;
                assert!(
                    message.contains(&format!("at line {authored_line} (")),
                    "authored line in the headline: {message}"
                );
                assert!(
                    message.contains(&format!("┌─ glsl:{authored_line}:")),
                    "authored line in the marker: {message}"
                );
            }
            other => panic!("expected GfxError::Compile, got {other:?}"),
        }
    }

    fn compile_error_message(authored: &str) -> String {
        match compile_wgsl_no_textures(authored) {
            Err(GfxError::Compile(message)) => message,
            Err(other) => panic!("expected GfxError::Compile, got {other:?}"),
            Ok(_) => panic!("must not compile:\n{authored}"),
        }
    }

    /// A parse error on authored line 3 reports line 3, although the
    /// assembled unit puts a version line, an lpfn prelude and a prototype
    /// in front of the authored text.
    #[test]
    fn parse_errors_are_reported_at_the_authored_line() {
        let message = compile_error_message(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) {\n\
                 float bad = ;\n\
                 return vec4(lpfn_saturate(pos.x));\n\
             }\n",
        );
        assert!(message.starts_with("naga glsl-in:"), "{message}");
        assert!(message.contains("┌─ glsl:3:"), "authored line: {message}");
        assert!(
            message.contains("3 │"),
            "the gutter shows the authored line too: {message}"
        );
    }

    /// A validation error carries the `glsl` label (naga's default for
    /// `WithSpan` is `wgsl`, which the Studio parser ignores) and an authored
    /// line.
    #[test]
    fn validation_errors_are_reported_at_the_authored_line() {
        // A scalar uniform array fails naga's uniform-buffer layout
        // validation after parsing cleanly
        // (`docs/debt/wgpu-refuses-scalar-uniform-arrays.md`).
        let message = compile_error_message(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             layout(binding = 1) uniform float weights[4];\n\
             vec4 render_2d(vec2 pos) { return vec4(weights[0]); }\n",
        );
        assert!(message.starts_with("naga validation:"), "{message}");
        assert!(!message.contains("┌─ wgsl:"), "{message}");
        assert!(message.contains("┌─ glsl:2:"), "authored line: {message}");
    }

    /// An error whose span lies outside the authored text (here, the wrapper
    /// `main` calling an entry the shader never defined) reports no location
    /// rather than a line past the end of the editor.
    #[test]
    fn errors_outside_the_authored_text_carry_no_location() {
        let message = compile_error_message("vec4 render_1d(float pos) { return vec4(pos); }\n");
        assert!(message.starts_with("naga glsl-in:"), "{message}");
        assert!(
            !message.contains("┌─ glsl:"),
            "no authored line for a wrapper span: {message}"
        );
    }

    #[test]
    fn bounded_loops_carry_the_invocation_budget_in_wgsl() {
        let shader = compile_wgsl_no_textures(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) {\n\
                 float a = 0.0;\n\
                 for (int i = 0; i < 8; i++) { a += pos.x / outputSize.x; }\n\
                 return vec4(a);\n\
             }\n",
        )
        .expect("a bounded loop translates");
        assert!(
            shader
                .wgsl
                .contains(&format!("var<private> {LOOP_BUDGET_GLOBAL}: u32 = 0u;")),
            "per-invocation budget global:\n{}",
            shader.wgsl
        );
        assert!(
            shader.wgsl.contains("break if"),
            "the loop breaks when the budget is spent:\n{}",
            shader.wgsl
        );
        assert!(
            shader
                .wgsl
                .contains(&format!("> {DEFAULT_INVOCATION_FUEL}u")),
            "the budget is the LPVM fuel tank:\n{}",
            shader.wgsl
        );
    }

    #[test]
    fn broken_shader_reports_a_compile_error_with_diagnostics() {
        let err =
            match compile_wgsl_no_textures("vec4 render_2d(vec2 pos) { return not_defined(pos); }")
            {
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
             vec4 render_2d(vec2 pos) { return late(pos); }\n\
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
             vec4 render_2d(vec2 pos) { return texelFetch(inputColor, ivec2(pos), 0); }\n",
            &textures,
            ShaderEntrySpace::TwoD,
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
             vec4 render_2d(vec2 pos) { return texture(t, pos / 8.0); }\n",
            &textures,
            ShaderEntrySpace::TwoD,
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
