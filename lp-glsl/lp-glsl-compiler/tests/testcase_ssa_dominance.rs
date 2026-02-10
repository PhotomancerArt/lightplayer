//! Test to reproduce SSA dominance violation bug
//!
//! This test isolates the issue where constants are created in blocks that don't
//! dominate their uses, causing SSA dominance violations.
//!
//! **Root Cause:**
//! The bug occurs when:
//! 1. Constants are created on-demand in whatever block they're first encountered during transformation
//! 2. Blocks are copied in layout order (not dominance order)
//! 3. A constant created in a later block (in layout order) is used in an earlier block
//! 4. When copying the earlier block, the constant isn't in `value_map` yet
//! 5. `map_operand()` in `helpers.rs` returns the old `Value` if not found, causing a dominance violation
//!
//! **Minimal Reproduction:**
//! The `test_exact_failing_shader` test reproduces the issue. The error shows:
//! - `v108 = iconst.i8 1` is created in `block9` (inst99)
//! - `v108` is used in `block2` (inst131) in a function call: `call fn38(v134, v108)`
//! - `block9` doesn't dominate `block2`, causing the violation
//!
//! The issue is that `map_operand()` in `helpers.rs` uses the buggy `map_value()` that returns
//! the old `Value` if not found, instead of returning an error. This should be fixed to match
//! the behavior of `map_value()` in `instruction_copy.rs`.

use lp_glsl_compiler::{DecimalFormat, GlslOptions, RunMode, glsl_jit};

#[test]
fn test_minimal_ssa_dominance_violation() {
    // Minimal shader that reproduces the issue:
    // - Has control flow (if-else chain)
    // - Uses constants in select instructions
    // - Constants are used in blocks that can be reached without going through
    //   the block where the constant was created

    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float hue = time * 0.5;
    
    vec3 rgb;
    if (hue < 1.0/6.0) {
        rgb = vec3(1.0, 0.0, 0.0);
    } else if (hue < 2.0/6.0) {
        rgb = vec3(0.0, 1.0, 0.0);
    } else {
        rgb = vec3(0.0, 0.0, 1.0);
    }
    
    return vec4(rgb, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success - compilation worked
        }
        Err(e) => {
            // Compilation error - check if it's the dominance violation
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map (this is the bug we're investigating): {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation (this is the bug): {e}");
            } else {
                panic!("Unexpected compilation error: {e}");
            }
        }
    }
}

#[test]
fn test_simple_if_else() {
    // Even simpler: just an if-else with constants
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    vec3 rgb;
    if (time < 0.5) {
        rgb = vec3(1.0, 0.0, 0.0);
    } else {
        rgb = vec3(0.0, 1.0, 0.0);
    }
    return vec4(rgb, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_select_with_constants() {
    // Test select instruction with constants directly
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float x = time < 0.5 ? 1.0 : 0.0;
    return vec4(x, x, x, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_multiple_if_else_branches() {
    // Reproduce the exact pattern from the failing test:
    // Multiple if-else branches where constants (like 0.0, 1.0) are used
    // across different branches. The constants may be created in one branch
    // but used in another, causing dominance violations.
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float hue = time * 0.5;
    
    vec3 rgb;
    if (hue < 1.0/6.0) {
        rgb = vec3(1.0, 0.0, 0.0);
    } else if (hue < 2.0/6.0) {
        rgb = vec3(0.0, 1.0, 0.0);
    } else if (hue < 3.0/6.0) {
        rgb = vec3(0.0, 0.0, 1.0);
    } else if (hue < 4.0/6.0) {
        rgb = vec3(0.0, 1.0, 1.0);
    } else if (hue < 5.0/6.0) {
        rgb = vec3(1.0, 0.0, 1.0);
    } else {
        rgb = vec3(1.0, 1.0, 0.0);
    }
    
    return vec4(rgb, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_arithmetic_with_constants_in_branches() {
    // Minimal reproduction: arithmetic operations with constants in different branches
    // The constants are created in one branch but used in function calls in another branch
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float x;
    if (time < 0.5) {
        x = 1.0 + 0.0;  // Creates constants and calls __lp_q32_add
    } else {
        x = 0.0 + 1.0;  // Uses constants 0.0 and 1.0 that may have been created in the other branch
    }
    return vec4(x, x, x, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_vec3_construction_with_constants() {
    // Reproduce: vec3 construction with constants in branches
    // vec3(c, x, 0.0) creates a vec3 which may involve function calls
    // The constant 0.0 might be created in one branch but used in another
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float c = time * 0.5;
    float x = c * 0.3;
    
    vec3 rgb;
    if (time < 0.5) {
        rgb = vec3(c, x, 0.0);  // Constant 0.0 used here
    } else {
        rgb = vec3(x, c, 0.0);  // Same constant 0.0 used here - may be created in wrong block
    }
    
    return vec4(rgb, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_multiple_branches_with_shared_constants() {
    // This test reproduces the exact pattern from the failing shader:
    // Multiple if-else branches where the same constants (0.0, 1.0) are used
    // in different branches. The constants may be created in a later branch but
    // used in an earlier branch (in terms of control flow), causing dominance violations.
    let glsl = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    float hue = time * 0.5;
    float c = 0.8;
    float x = 0.3;
    
    vec3 rgb;
    if (hue < 1.0/6.0) {
        rgb = vec3(c, x, 0.0);  // Constant 0.0 - first use
    } else if (hue < 2.0/6.0) {
        rgb = vec3(x, c, 0.0);  // Constant 0.0 - second use
    } else if (hue < 3.0/6.0) {
        rgb = vec3(0.0, c, x);  // Constant 0.0 - third use, different position
    } else if (hue < 4.0/6.0) {
        rgb = vec3(0.0, x, c);  // Constant 0.0 - fourth use
    } else if (hue < 5.0/6.0) {
        rgb = vec3(x, 0.0, c);  // Constant 0.0 - fifth use
    } else {
        rgb = vec3(c, 0.0, x);  // Constant 0.0 - sixth use
    }
    
    return vec4(rgb, 1.0);
}
"#;

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}

#[test]
fn test_exact_failing_shader() {
    // This is the exact shader from testcase_reloc_panic.rs that's failing
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

    let options_q32 = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Q32,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    let result = glsl_jit(glsl, options_q32);

    match result {
        Ok(_executable) => {
            // Success
        }
        Err(e) => {
            let error_msg = format!("{e}");
            if error_msg.contains("not found in value_map") {
                panic!("Value not found in value_map: {e}");
            } else if error_msg.contains("non-dominating") {
                panic!("SSA dominance violation: {e}");
            } else {
                panic!("Unexpected error: {e}");
            }
        }
    }
}
