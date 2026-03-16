//! Integration tests for streaming per-function compilation.
//!
//! Verifies that glsl_jit_streaming produces the same results as glsl_jit
//! for real-world shaders.

use lp_glsl_cranelift::{GlslOptions, GlslValue, execute_function, glsl_jit, glsl_jit_streaming};

fn q32_jit_options() -> GlslOptions {
    let mut opts = GlslOptions::jit();
    opts.decimal_format = lp_glsl_cranelift::DecimalFormat::Q32;
    opts
}

#[test]
fn test_streaming_matches_batch_rainbow_shader() {
    let source = include_str!("../../../examples/basic/src/rainbow.shader/main.glsl");
    let options = q32_jit_options();

    let mut streaming =
        glsl_jit_streaming(source, options.clone()).expect("streaming compilation failed");
    let mut batch = glsl_jit(source, options).expect("batch compilation failed");

    assert!(streaming.get_direct_call_info("main").is_some());
    assert!(batch.get_direct_call_info("main").is_some());

    let args = [
        GlslValue::Vec2([100.0, 200.0]),
        GlslValue::Vec2([256.0, 256.0]),
        GlslValue::F32(1.0),
    ];

    let streaming_result = execute_function(&mut *streaming, "main", &args).unwrap();
    let batch_result = execute_function(&mut *batch, "main", &args).unwrap();

    assert!(
        streaming_result.approx_eq(&batch_result, 0.01),
        "streaming and batch should produce the same result"
    );
}

#[test]
fn test_streaming_matches_batch_multi_function() {
    let source = r#"
        vec3 palette(float t) {
            vec3 r = t * 2.1 - vec3(1.8, 1.14, 0.3);
            return clamp(1.0 - r * r, 0.0, 1.0);
        }
        vec3 apply(float t, float blend) {
            return mix(palette(t), palette(t + 0.1), blend);
        }
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            float t = fragCoord.x / outputSize.x;
            vec3 rgb = apply(t, 0.5);
            return vec4(rgb, 1.0);
        }
    "#;
    let options = q32_jit_options();

    let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
    let mut batch = glsl_jit(source, options).unwrap();

    assert!(streaming.get_direct_call_info("main").is_some());
    assert!(batch.get_direct_call_info("main").is_some());

    let args = [
        GlslValue::Vec2([0.0, 0.0]),
        GlslValue::Vec2([256.0, 256.0]),
        GlslValue::F32(0.0),
    ];

    let streaming_result = execute_function(&mut *streaming, "main", &args).unwrap();
    let batch_result = execute_function(&mut *batch, "main", &args).unwrap();

    assert!(
        streaming_result.approx_eq(&batch_result, 0.01),
        "streaming and batch should produce the same result"
    );
}
