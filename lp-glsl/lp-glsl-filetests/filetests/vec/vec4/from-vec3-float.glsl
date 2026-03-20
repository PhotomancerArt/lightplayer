// test run

// ============================================================================
// vec4(vec3, float) lengthening constructor
// ============================================================================

vec4 test_vec4_from_vec3_float() {
    vec3 rgb = vec3(0.25, 0.5, 0.75);
    return vec4(rgb, 1.0);
}

// run: test_vec4_from_vec3_float() ~= vec4(0.25, 0.5, 0.75, 1.0)

vec4 test_vec4_from_vec3_float_expr() {
    return vec4(vec3(1.0, 0.0, 0.0) * 0.5, 1.0);
}

// run: test_vec4_from_vec3_float_expr() ~= vec4(0.5, 0.0, 0.0, 1.0)
