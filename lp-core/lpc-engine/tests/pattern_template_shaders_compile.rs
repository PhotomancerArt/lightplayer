//! The New menu's pattern templates must ship shaders that **compile**.
//!
//! A template whose `effect/shader.glsl` does not compile renders black,
//! and a shader compile failure is exactly the class of defect that hides:
//! the node keeps its last-good program and the failure lands on a status
//! nobody is looking at (`docs/defects/…events-render-flake…`). The
//! project loader does not compile shaders — it resolves refs and wires
//! bindings — so loading a template proves nothing about its body.
//!
//! This runs the templates' GLSL through the same call the shader node
//! makes, `LpGraphics::compile_shader` on the CPU (lpvm) tier, at the
//! authored `Fixed` numerics. It lives in `lpc-engine` rather than beside
//! the composition because `lpc-model` carries no compiler.

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lpc_model::{SlotShapeRegistry, pattern_project_files_1d, pattern_project_files_2d};

/// The template's `effect/shader.glsl`, as authored.
fn template_shader(files: &[(String, Vec<u8>)]) -> String {
    files
        .iter()
        .find(|(path, _)| path == "effect/shader.glsl")
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8 GLSL"))
        .expect("the template ships effect/shader.glsl")
}

#[test]
fn both_pattern_template_shaders_compile_on_the_cpu_tier() {
    let registry = SlotShapeRegistry::default();
    let templates = [
        (
            "1D",
            template_shader(&pattern_project_files_1d("demo", &registry).expect("1d")),
        ),
        (
            "2D",
            template_shader(&pattern_project_files_2d("demo", &registry).expect("2d")),
        ),
    ];

    // The tier the starter shaders are authored for: `FloatMode::Fixed` on
    // the CPU backend, which is what the editor sim and every classic
    // device run.
    let graphics = lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl);
    let options =
        ShaderCompileOptions::new(ShaderSemantics::Q32, lp_shader::ShaderFrontend::LpsGlsl);

    for (name, glsl) in templates {
        graphics
            .compile_shader(&glsl, &options)
            .unwrap_or_else(|error| {
                panic!("the {name} pattern template's shader must compile: {error}\n{glsl}")
            });
    }
}
