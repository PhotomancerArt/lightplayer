// test run
// target riscv32.q32

// ============================================================================
// lpfx_saturate(): Clamp values between 0 and 1
// ============================================================================

float test_lpfx_saturate_below_zero() {
    // Negative values should clamp to 0
    float result = lpfx_saturate(-0.5);
    return abs(result - 0.0) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_saturate_below_zero() == 1.0

float test_lpfx_saturate_above_one() {
    // Values above 1 should clamp to 1
    float result = lpfx_saturate(1.5);
    return abs(result - 1.0) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_saturate_above_one() == 1.0

float test_lpfx_saturate_in_range() {
    // Values in [0, 1] should remain unchanged
    float result = lpfx_saturate(0.5);
    return abs(result - 0.5) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_saturate_in_range() == 1.0

float test_lpfx_saturate_vec3() {
    // Test vec3 saturation
    vec3 v = vec3(-0.5, 0.5, 1.5);
    vec3 result = lpfx_saturate(v);
    bool valid = abs(result.x - 0.0) < 0.01 &&
                 abs(result.y - 0.5) < 0.01 &&
                 abs(result.z - 1.0) < 0.01;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_saturate_vec3() == 1.0

float test_lpfx_saturate_vec4() {
    // Test vec4 saturation
    vec4 v = vec4(-0.5, 0.5, 1.5, 0.25);
    vec4 result = lpfx_saturate(v);
    bool valid = abs(result.x - 0.0) < 0.01 &&
                 abs(result.y - 0.5) < 0.01 &&
                 abs(result.z - 1.0) < 0.01 &&
                 abs(result.w - 0.25) < 0.01;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_saturate_vec4() == 1.0
