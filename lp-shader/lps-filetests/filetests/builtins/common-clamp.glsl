// test run

// ============================================================================
// clamp(): Constrain to [minVal, maxVal]
// ============================================================================

float test_clamp_float_high() {
    return clamp(5.0, 0.0, 1.0);
}

// run: test_clamp_float_high() ~= 1.0

float test_clamp_float_low() {
    return clamp(-2.0, 0.0, 1.0);
}

// run: test_clamp_float_low() ~= 0.0

float test_clamp_float_inside() {
    return clamp(0.25, 0.0, 1.0);
}

// run: test_clamp_float_inside() ~= 0.25

vec3 test_clamp_vec3_scalar_bounds() {
    return clamp(vec3(1.5, -1.0, 0.5), 0.0, 1.0);
}

// run: test_clamp_vec3_scalar_bounds() ~= vec3(1.0, 0.0, 0.5)
