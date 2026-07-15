//! `ShaderCompileOptions::textures` is the compile-time texture contract
//! shared with the GPU tier: the CPU backend threads it into
//! `CompilePxDesc`, so sampler shaders compile with a matching spec and
//! fail without one (fail-fast validation, lp-shader-texture-access).

use lp_gfx::{GfxError, LpGraphics, ShaderCompileOptions};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::texture_binding;
use lps_shared::{TextureFilter, TextureStorageFormat, TextureWrap};

const TEXTURE_SHADER: &str = "uniform sampler2D inputColor;\n\
                              vec4 render(vec2 pos) { return texelFetch(inputColor, ivec2(pos), 0); }\n";

#[test]
fn texture_shader_compiles_when_the_spec_map_matches() {
    let graphics = TargetLpvmGraphics::new();
    let mut options = ShaderCompileOptions::default();
    // Pin the frontend: the default flips to Naga under feature unification
    // (lp-gfx-wgpu enables lp-shader/naga), and lps-frontend's texelFetch
    // lowering currently rejects constructor-typed coordinates
    // (lower_texture.rs "coordinate must be ivec2") — a tracked parity gap.
    options.frontend = lp_shader::ShaderFrontend::LpsGlsl;
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
        .expect("sampler shader compiles once the spec map is threaded through");
}

#[test]
fn texture_shader_fails_without_a_spec() {
    let graphics = TargetLpvmGraphics::new();
    let mut options = ShaderCompileOptions::default();
    options.frontend = lp_shader::ShaderFrontend::LpsGlsl;
    match graphics.compile_shader(TEXTURE_SHADER, &options) {
        Err(GfxError::Compile(message)) => {
            assert!(
                message.contains("inputColor"),
                "error names the sampler: {message}"
            );
        }
        Err(other) => panic!("expected GfxError::Compile, got {other:?}"),
        Ok(_) => panic!("missing spec must fail compilation"),
    }
}
