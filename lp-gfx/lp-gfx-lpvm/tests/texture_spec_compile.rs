//! `ShaderCompileOptions::textures` is the compile-time texture contract
//! shared with the GPU tier: the CPU backend threads it into
//! `CompilePxDesc`, so sampler shaders compile with a matching spec and
//! fail without one (fail-fast validation, lp-shader-texture-access).
//!
//! The constructor-typed coordinate `ivec2(pos)` is a frontend parity
//! point (lps-frontend used to reject it): both frontends accept it now.
//! Naga is exercised only when `lp-shader/naga` is built into this test
//! binary (probed at runtime — frontend selection is explicit, but the
//! naga code itself is still feature-gated); dedicated naga coverage
//! lives in lps-frontend's unit tests and the texture filetests.

use lp_gfx::{GfxError, LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::{ShaderFrontend, texture_binding};
use lps_shared::{TextureFilter, TextureStorageFormat, TextureWrap};

const TEXTURE_SHADER: &str = "uniform sampler2D inputColor;\n\
                              vec4 render(vec2 pos) { return texelFetch(inputColor, ivec2(pos), 0); }\n";

/// LpsGlsl always; Naga when compiled in (some crate in the test build graph
/// enables `lp-shader/naga` — probe rather than guess).
fn available_frontends() -> Vec<ShaderFrontend> {
    let mut list = vec![ShaderFrontend::LpsGlsl];
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::Naga);
    let options = ShaderCompileOptions::new(ShaderSemantics::Q32, ShaderFrontend::Naga);
    match graphics.compile_shader("vec4 render(vec2 pos) { return vec4(0.0); }", &options) {
        Err(GfxError::Compile(m)) if m.contains("naga frontend was not built") => {}
        _ => list.push(ShaderFrontend::Naga),
    }
    list
}

#[test]
fn texture_shader_compiles_when_the_spec_map_matches() {
    for frontend in available_frontends() {
        let graphics = TargetLpvmGraphics::new(frontend);
        let mut options = ShaderCompileOptions::new(ShaderSemantics::Q32, frontend);
        options.textures.insert(
            String::from("inputColor"),
            texture_binding::texture2d(
                TextureStorageFormat::Rgba16Unorm,
                TextureFilter::Nearest,
                TextureWrap::ClampToEdge,
                TextureWrap::ClampToEdge,
            ),
        );
        graphics
            .compile_shader(TEXTURE_SHADER, &options)
            .unwrap_or_else(|e| {
                panic!("sampler shader compiles with a matching spec ({frontend:?}): {e:?}")
            });
    }
}

#[test]
fn texture_shader_fails_without_a_spec() {
    for frontend in available_frontends() {
        let graphics = TargetLpvmGraphics::new(frontend);
        let options = ShaderCompileOptions::new(ShaderSemantics::Q32, frontend);
        match graphics.compile_shader(TEXTURE_SHADER, &options) {
            Err(GfxError::Compile(message)) => {
                assert!(
                    message.contains("inputColor"),
                    "error names the sampler ({frontend:?}): {message}"
                );
            }
            Err(other) => panic!("expected GfxError::Compile ({frontend:?}), got {other:?}"),
            Ok(_) => panic!("missing spec must fail compilation ({frontend:?})"),
        }
    }
}
