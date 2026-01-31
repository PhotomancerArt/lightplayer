// test run
// target riscv32.q32

// ============================================================================
// atan(): Arc tangent function
// atan(y_over_x) - Range: [-π/2, π/2]
// atan(y, x) - Range: [-π, π], determines quadrant
// Undefined if both x and y are 0 (two-arg version)
// ============================================================================

float test_atan_zero() {
    // atan(0) should be 0
    return atan(0.0);
}

// run: test_atan_zero() ~= 0.0

float test_atan_one() {
    // atan(1) should be π/4
    return atan(1.0);
}

// run: test_atan_one() ~= 0.7853981633974483

float test_atan_neg_one() {
    // atan(-1) should be -π/4
    return atan(-1.0);
}

// run: test_atan_neg_one() ~= -0.7853981633974483

float test_atan_large() {
    // atan(very large) should be π/2
    return atan(1e10);
}

// run: test_atan_large() ~= 1.5707963267948966 (tolerance: 1.6)

float test_atan_neg_large() {
    // atan(very large negative) should be -π/2
    return atan(-1e10);
}

// run: test_atan_neg_large() ~= -1.5707963267948966 (tolerance: 1.6)

// Two-argument versions
float test_atan2_first_quadrant() {
    // atan(1, 1) should be π/4
    return atan(1.0, 1.0);
}

// run: test_atan2_first_quadrant() ~= 0.7853981633974483

float test_atan2_second_quadrant() {
    // atan(1, -1) should be 3π/4
    return atan(1.0, -1.0);
}

// run: test_atan2_second_quadrant() ~= 2.356194490192345

float test_atan2_third_quadrant() {
    // atan(-1, -1) should be -3π/4
    return atan(-1.0, -1.0);
}

// run: test_atan2_third_quadrant() ~= -2.356194490192345

float test_atan2_fourth_quadrant() {
    // atan(-1, 1) should be -π/4
    return atan(-1.0, 1.0);
}

// run: test_atan2_fourth_quadrant() ~= -0.7853981633974483

vec2 test_atan_vec2() {
    // Test single argument with vec2
    return atan(vec2(0.0, 1.0));
}

// run: test_atan_vec2() ~= vec2(0.0, 0.7853981633974483)

vec3 test_atan_vec3() {
    // Test single argument with vec3
    return atan(vec3(0.0, 1.0, -1.0));
}

// run: test_atan_vec3() ~= vec3(0.0, 0.7853981633974483, -0.7853981633974483)



