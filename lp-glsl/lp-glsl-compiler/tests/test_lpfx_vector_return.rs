//! Test LPFX functions with vector return types
//!
//! This test exercises both emulator and JIT execution paths for LPFX functions
//! that return vectors (using result pointer parameters). This helps debug
//! pointer type mismatches between architectures (i32 on RISC-V 32-bit, i64 on native JIT).

#[cfg(feature = "emulator")]
use lp_glsl_compiler::glsl_emu_riscv32;
use lp_glsl_compiler::{DecimalFormat, GlslOptions, GlslValue, Q32Options, RunMode, glsl_jit};

/// Test lpfx_hsv2rgb with vec3 return (result pointer parameter) in JIT mode
#[test]
fn test_lpfx_hsv2rgb_jit() {
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    vec3 hsv = vec3(0.5, 1.0, 1.0); // Hue=0.5 (cyan), Saturation=1.0, Value=1.0
    // THIS IS THE RESULT POINTER PARAMETER CALL - tests pointer type matching
    vec3 rgb = lpfx_hsv2rgb(hsv);
    return vec4(rgb, 1.0);
}
"#;

    let options = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: Q32Options::default(),
    };

    // Compile and execute
    let result = glsl_jit(glsl, options);

    match result {
        Ok(mut executable) => {
            // DEBUG: Print CLIF/v-code if available
            println!("=== DEBUG: Checking compiled function ===");
            let (_original_clif, transformed_clif) = executable.format_clif_ir();
            if let Some(ref clif) = transformed_clif {
                println!("=== Transformed CLIF IR ===\n{}", clif);
            }
            if let Some(ref vcode) = executable.format_vcode() {
                println!("=== VCode ===\n{}", vcode);
            }
            if let Some(ref disasm) = executable.format_disassembly() {
                println!("=== Disassembly ===\n{}", disasm);
            }

            // Call main with test arguments
            let frag_coord = GlslValue::Vec2([100.0, 100.0]);
            let output_size = GlslValue::Vec2([200.0, 200.0]);
            let time = GlslValue::F32(0.0);

            let main_result = executable.call_vec("main", &[frag_coord, output_size, time], 4);

            match main_result {
                Ok(result) => {
                    println!("JIT: main() returned vec4({:?})", result);
                    // Extract RGB from result (result[0..3] should be the RGB from lpfx_hsv2rgb)
                    let rgb = [result[0], result[1], result[2]];
                    println!("JIT: lpfx_hsv2rgb(vec3(0.5, 1.0, 1.0)) = vec3({:?})", rgb);
                    // Expected: cyan (hue=0.5) should be approximately (0.0, 1.0, 1.0)
                    // Allow some tolerance for fixed-point arithmetic
                    assert!(
                        (rgb[0] - 0.0).abs() < 0.1,
                        "R component should be ~0.0, got {}",
                        rgb[0]
                    );
                    assert!(
                        (rgb[1] - 1.0).abs() < 0.1,
                        "G component should be ~1.0, got {}",
                        rgb[1]
                    );
                    assert!(
                        (rgb[2] - 1.0).abs() < 0.1,
                        "B component should be ~1.0, got {}",
                        rgb[2]
                    );
                }
                Err(e) => {
                    panic!("JIT execution failed: {:#}", e);
                }
            }
        }
        Err(e) => {
            panic!("JIT compilation failed: {:#}", e);
        }
    }
}

/// Test lpfx_hsv2rgb with vec3 return (result pointer parameter) in emulator mode
#[cfg(feature = "emulator")]
#[test]
fn test_lpfx_hsv2rgb_emulator() {
    let glsl = r#"
vec3 test_hsv2rgb(vec3 hsv) {
    return lpfx_hsv2rgb(hsv);
}

vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    vec3 hsv = vec3(0.5, 1.0, 1.0); // Hue=0.5 (cyan), Saturation=1.0, Value=1.0
    vec3 rgb = test_hsv2rgb(hsv);
    return vec4(rgb, 1.0);
}
"#;

    let options = GlslOptions {
        run_mode: RunMode::Emulator {
            max_memory: 1024 * 1024,
            stack_size: 64 * 1024,
            max_instructions: 1000000,
            log_level: None,
        },
        decimal_format: DecimalFormat::Q32,
        q32_opts: Q32Options::default(),
    };

    // Compile and execute
    let result = glsl_emu_riscv32(glsl, options);

    match result {
        Ok(mut executable) => {
            // Call the test function directly
            let hsv = GlslValue::Vec3([0.5, 1.0, 1.0]);
            let rgb_result = executable.call_vec("test_hsv2rgb", &[hsv], 3);

            match rgb_result {
                Ok(rgb) => {
                    println!(
                        "Emulator: lpfx_hsv2rgb(vec3(0.5, 1.0, 1.0)) = vec3({:?})",
                        rgb
                    );
                    // Expected: cyan (hue=0.5) should be approximately (0.0, 1.0, 1.0)
                    // Allow some tolerance for fixed-point arithmetic
                    assert!(
                        (rgb[0] - 0.0).abs() < 0.1,
                        "R component should be ~0.0, got {}",
                        rgb[0]
                    );
                    assert!(
                        (rgb[1] - 1.0).abs() < 0.1,
                        "G component should be ~1.0, got {}",
                        rgb[1]
                    );
                    assert!(
                        (rgb[2] - 1.0).abs() < 0.1,
                        "B component should be ~1.0, got {}",
                        rgb[2]
                    );
                }
                Err(e) => {
                    panic!("Emulator execution failed: {:#}", e);
                }
            }
        }
        Err(e) => {
            panic!("Emulator compilation failed: {:#}", e);
        }
    }
}

/// Test that exercises the exact pattern from the user's shader
#[test]
fn test_rainbow_shader_pattern_jit() {
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Pan through noise using time with oscillation to stay bounded
    float panSpeed = 0.3;
    float pan = mix(1.0, 8.0, 0.5 * (sin(time * panSpeed) + 1.0));
    
    float scaleSpeed = 0.7;
    float scale = mix(0.02, 0.06, 0.5 * (sin(time * scaleSpeed) + 1.0));
    
    // Scale from center: translate to center, scale, translate back
    vec2 center = outputSize * 0.5;
    vec2 dir = fragCoord - center;
    vec2 scaledCoord = center + dir * scale;
    
    // Sample Simplex noise with zoom using LP library function
    float noiseValue = lpfx_snoise(vec3(scaledCoord, pan), 0u);
    float hue = cos(noiseValue * 3.1415 + time) / 2.0 + 0.5;
    
    // Convert HSV to RGB - THIS IS THE RESULT POINTER PARAMETER CALL
    vec3 rgb = lpfx_hsv2rgb(vec3(hue, 1.0, 1.0));
    
    // Clamp to [0, 1] and return
    return vec4(rgb, 1.0);
}
"#;

    let options = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: Q32Options::default(),
    };

    // Compile and execute
    let result = glsl_jit(glsl, options);

    match result {
        Ok(mut executable) => {
            // Call main with some test arguments
            let frag_coord = GlslValue::Vec2([100.0, 100.0]);
            let output_size = GlslValue::Vec2([200.0, 200.0]);
            let time = GlslValue::F32(1.0);

            let main_result = executable.call_vec("main", &[frag_coord, output_size, time], 4);

            match main_result {
                Ok(result) => {
                    println!("JIT: main() returned vec4({:?})", result);
                    // Just verify it doesn't crash - the actual values depend on noise
                    assert_eq!(result.len(), 4);
                }
                Err(e) => {
                    panic!("JIT execution failed: {:#}", e);
                }
            }
        }
        Err(e) => {
            panic!("JIT compilation failed: {:#}", e);
        }
    }
}
