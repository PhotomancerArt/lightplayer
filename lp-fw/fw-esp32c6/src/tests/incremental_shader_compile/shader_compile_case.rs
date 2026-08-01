//! Representative compile corpus for the incremental shader compile harness.

use lp_shader::{CompilePxDesc, ShaderFrontend, TextureStorageFormat, texture_binding};

pub struct ShaderCompileCase {
    pub name: &'static str,
    pub glsl: &'static str,
    pub with_input_color: bool,
}

pub const SHADER_COMPILE_CASES: &[ShaderCompileCase] = &[ShaderCompileCase {
    name: "examples-basic",
    glsl: include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/basic/shader.glsl"
    )),
    with_input_color: false,
}];

impl ShaderCompileCase {
    pub fn desc(&self) -> CompilePxDesc<'static> {
        // LpsGlsl: the incremental (stepped) frontend path only exists for
        // the device pipeline's native frontend; naga completes in one step.
        let desc = CompilePxDesc::new(
            self.glsl,
            TextureStorageFormat::Rgba16Unorm,
            lpir::CompilerConfig::default(),
            ShaderFrontend::LpsGlsl,
        );
        if self.with_input_color {
            desc.with_texture_spec(
                "inputColor",
                texture_binding::texture2d(
                    TextureStorageFormat::Rgba16Unorm,
                    lps_shared::TextureFilter::Nearest,
                    lps_shared::TextureWrap::ClampToEdge,
                    lps_shared::TextureWrap::ClampToEdge,
                ),
            )
        } else {
            desc
        }
    }
}
