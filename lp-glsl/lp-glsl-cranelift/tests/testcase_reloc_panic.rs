//! Test to reproduce the TestCase relocation panic on macOS
//!
//! This test reproduces the exact GLSL shader from the default project
//! that causes a panic when compiling on macOS due to unimplemented
//! TestCase relocation handling.

use lp_glsl_cranelift::{
    glsl_jit, FloatMode, GlslOptions, Q32Options, RunMode, DEFAULT_MAX_ERRORS,
};

#[test]
fn test_default_project_shader_compilation() {
    // This is the exact GLSL shader from LpApp::create_default_project()
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Center of texture
    vec2 center = outputSize * 0.5;
    
    // Direction from center to fragment
    vec2 dir = fragCoord - center;
    
    // Calculate angle (atan2 gives angle in [-PI, PI])
    float angle = atan(dir.y, dir.x);
    
    // Rotate angle with time (full rotation every 4 seconds)
    angle = angle + time * 0.5;
    
    // Normalize angle to [0, 1] for hue
    float hue = (angle / (2.0 * 3.14159) + 1.0) * 0.5;
    
    // Distance from center (normalized)
    float dist = length(dir) / (min(outputSize.x, outputSize.y) * 0.5);
    
    // Create color wheel: hue rotates, saturation and value vary with distance
    // Convert HSV to RGB (simplified)
    float c = 1.0 - abs(dist - 0.5) * 2.0; // Saturation based on distance
    float x = c * (1.0 - abs(mod(hue * 6.0, 2.0) - 1.0));
    float m = 0.8 - dist * 0.3; // Value (brightness)
    
    vec3 rgb;
    if (hue < 1.0/6.0) {
        rgb = vec3(c, x, 0.0);
    } else if (hue < 2.0/6.0) {
        rgb = vec3(x, c, 0.0);
    } else if (hue < 3.0/6.0) {
        rgb = vec3(0.0, c, x);
    } else if (hue < 4.0/6.0) {
        rgb = vec3(0.0, x, c);
    } else if (hue < 5.0/6.0) {
        rgb = vec3(x, 0.0, c);
    } else {
        rgb = vec3(c, 0.0, x);
    }
    
    return vec4((rgb + m - 0.4) * m, 1.0);
}
"#;

    // Test with Q32 format (the only supported format)
    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        float_mode: FloatMode::Q32,
        q32_opts: Q32Options::default(),
        memory_optimized: false,
        target_override: None,
        max_errors: DEFAULT_MAX_ERRORS,
    };

    // This should not panic - Q32 direct emission uses builtins; TestCase relocations
    // are resolved via symbol_lookup_fn (map_testcase_to_builtin) when they occur.
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| glsl_jit(glsl, options_q32)));

    match result {
        Ok(Ok(_executable)) => {
            // Success - compilation worked with Q32 format
            // This confirms the bug is fixed
        }
        Ok(Err(e)) => {
            // Compilation error - this is unexpected but not a panic
            panic!("GLSL compilation failed (unexpected): {e}");
        }
        Err(_) => {
            // Panic occurred - this is the bug we're trying to fix
            panic!("GLSL compilation panicked - this is the bug we need to fix!");
        }
    }

    // Float format: on x86 HostJit it may succeed (TestCase relocations resolved via symbol lookup).
    // On RISC-V 32-bit HostJit it is rejected. Either outcome is acceptable; the important
    // fix was that Q32 no longer panics on TestCase relocations.
    let options_float = GlslOptions {
        run_mode: RunMode::HostJit,
        float_mode: FloatMode::Float,
        q32_opts: Q32Options::default(),
        memory_optimized: false,
        target_override: None,
        max_errors: DEFAULT_MAX_ERRORS,
    };

    match glsl_jit(glsl, options_float) {
        Ok(_) => {}
        Err(diagnostics) => {
            let msg = diagnostics
                .errors
                .first()
                .map(|e| e.message.as_str())
                .unwrap_or("");
            assert!(
                msg.contains("Float format") || msg.contains("not supported"),
                "If Float fails, error should mention format/support, got: {}",
                msg
            );
        }
    }
}
