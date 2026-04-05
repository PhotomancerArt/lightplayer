// test run

// ============================================================================
// smoothstep(): Hermite interpolation between edge0 and edge1
// ============================================================================

float test_smoothstep_scalar_mid() {
    return smoothstep(4.0, 5.0, 4.5);
}

// run: test_smoothstep_scalar_mid() ~= 0.5

float test_smoothstep_scalar_below() {
    return smoothstep(0.0, 1.0, -0.5);
}

// run: test_smoothstep_scalar_below() ~= 0.0

float test_smoothstep_scalar_above() {
    return smoothstep(0.0, 1.0, 2.0);
}

// run: test_smoothstep_scalar_above() ~= 1.0

vec2 test_smoothstep_vec2() {
    return smoothstep(vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.25, 0.75));
}

// run: test_smoothstep_vec2() ~= vec2(0.15625, 0.84375)

vec3 test_smoothstep_vec3() {
    return smoothstep(vec3(0.0), vec3(1.0), vec3(0.5, 0.0, 1.0));
}

// run: test_smoothstep_vec3() ~= vec3(0.5, 0.0, 1.0)
