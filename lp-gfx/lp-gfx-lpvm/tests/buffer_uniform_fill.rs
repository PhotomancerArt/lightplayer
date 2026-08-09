//! A VISUAL shader consuming a packed buffer uniform — the consumed-side
//! half of the buffer slot contract (`docs/adr/2026-08-08-typed-shader-
//! buffers.md`), at the layer the engine's shader node drives: compile the
//! declared `uniform float heat[N];`, fill it from `LpsValueF32::Buffer`
//! words through `set_uniform`, and read the rendered result back.
//!
//! Runtime indexing is the point: the fragment position picks the element,
//! so a wrong stride or a dropped word is a wrong pixel, not a compile
//! error.

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::ShaderFrontend;
use lps_shared::{LpsBuffer, LpsBufferElem, LpsValueF32};

const SHADER: &str = r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float heat[4];

vec4 render_2d(vec2 pos) {
    int i = int(pos.x);
    return vec4(heat[i], 0.0, 0.0, 1.0);
}
"#;

#[test]
fn buffer_words_fill_a_visual_uniform_array_runtime_indexed() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let options = ShaderCompileOptions::new(graphics.native_semantics(), ShaderFrontend::LpsGlsl);
    let mut shader = graphics
        .compile_shader(SHADER, &options)
        .expect("uniform float[4] compiles on the CPU tier");

    let heat = [0.25f32, 0.5, 0.75, 1.0];
    let buffer = LpsBuffer::from_words(
        LpsBufferElem::F32,
        heat.iter().map(|v| v.to_bits()).collect(),
    )
    .expect("buffer");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![
            (String::from("outputSize"), LpsValueF32::Vec2([4.0, 1.0])),
            (String::from("heat"), LpsValueF32::Buffer(buffer)),
        ],
    };

    let mut target = graphics.create_render_target(4, 1).expect("render target");
    shader.render(&mut target, &uniforms).expect("render");
    let data = graphics.read_back(&target).expect("read back");
    let bytes = data.into_bytes();

    // Rgba16Unorm, 8 bytes per pixel; red is the first u16.
    for (i, expected) in heat.iter().enumerate() {
        let at = i * 8;
        let red = u16::from_le_bytes([bytes[at], bytes[at + 1]]) as f32 / 65535.0;
        assert!(
            (red - expected).abs() < 0.01,
            "pixel {i}: red {red}, expected {expected}"
        );
    }
}
