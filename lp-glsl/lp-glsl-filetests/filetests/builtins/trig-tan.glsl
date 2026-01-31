// test run
// target riscv32.q32

// ============================================================================
// tan(): Tangent function
// ============================================================================

float test_tan_zero() {
    // tan(0) should be 0
    return tan(0.0);
}

// run: test_tan_zero() ~= 0.0

float test_tan_pi_fourth() {
    // tan(π/4) should be 1
    return tan(0.7853981633974483);
}

// run: test_tan_pi_fourth() ~= 1.0

float test_tan_pi_half() {
    // tan(π/2) should be undefined (very large positive)
    return tan(1.5707963267948966);
}

// run: test_tan_pi_half() ~= 1.6331778728383844e16 (tolerance: 1e17)

float test_tan_pi() {
    // tan(π) should be 0
    return tan(3.141592653589793);
}

// run: test_tan_pi() ~= 0.0 (tolerance: 0.01)

float test_tan_negative() {
    // tan(-π/4) should be -1
    return tan(-0.7853981633974483);
}

// run: test_tan_negative() ~= -1.0

float test_tan_small() {
    // tan(0.1) should be approximately 0.10033467208545055
    return tan(0.1);
}

// run: test_tan_small() ~= 0.10033467208545055

vec2 test_tan_vec2() {
    // Test with vec2
    return tan(vec2(0.0, 0.7853981633974483));
}

// run: test_tan_vec2() ~= vec2(0.0, 1.0)

vec3 test_tan_vec3() {
    // Test with vec3
    return tan(vec3(0.0, 0.7853981633974483, 3.141592653589793));
}

// run: test_tan_vec3() ~= vec3(0.0, 1.0, 0.0) (tolerance: 0.01)

vec4 test_tan_vec4() {
    // Test with vec4
    return tan(vec4(0.0, 0.7853981633974483, 3.141592653589793, -0.7853981633974483));
}

// run: test_tan_vec4() ~= vec4(0.0, 1.0, 0.0, -1.0) (tolerance: 0.01)



