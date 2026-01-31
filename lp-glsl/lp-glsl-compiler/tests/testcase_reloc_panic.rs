//! Test to reproduce the TestCase relocation panic on macOS
//!
//! This test reproduces the exact GLSL shader from the default project
//! that causes a panic when compiling on macOS due to unimplemented
//! TestCase relocation handling.

use lp_glsl_compiler::{DecimalFormat, GlslOptions, RunMode, glsl_jit};

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
        decimal_format: DecimalFormat::Q32,
    };

    // This should not panic - Q32 format goes through transform that converts TestCase names
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

    // Test that Float format is rejected with a clear error
    let options_float = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Float,
    };

    match glsl_jit(glsl, options_float) {
        Ok(_) => {
            panic!("Float format should be rejected with an error");
        }
        Err(e) => {
            // Expected error - Float format is not supported
            assert!(
                e.message.contains("Float format is not yet supported"),
                "Error message should mention Float format is not supported, got: {}",
                e.message
            );
        }
    }
}
