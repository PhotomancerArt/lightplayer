// test run

// ============================================================================
// Scalar * vec3 and vec3 * scalar (rainbow palettes use float * vec3)
// ============================================================================

vec3 test_scalar_mul_vec3_left() {
    float s = 2.1;
    vec3 v = vec3(1.8, 1.14, 0.3);
    return s * v;
}

// run: test_scalar_mul_vec3_left() ~= vec3(3.78, 2.394, 0.63)

vec3 test_scalar_mul_vec3_right() {
    return vec3(1.0, 2.0, 3.0) * 0.5;
}

// run: test_scalar_mul_vec3_right() ~= vec3(0.5, 1.0, 1.5)

vec3 test_scalar_mul_vec3_expr() {
    return 3.0 * (vec3(1.0, 1.0, 1.0) * 0.25);
}

// run: test_scalar_mul_vec3_expr() ~= vec3(0.75, 0.75, 0.75)
