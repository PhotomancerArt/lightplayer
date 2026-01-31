// test run
// target riscv32.q32

// ============================================================================
// cos(): Cosine function
// ============================================================================

float test_cos_zero() {
    // cos(0) should be 1
    return cos(0.0);
}

// run: test_cos_zero() ~= 1.0

float test_cos_pi_half() {
    // cos(π/2) should be 0
    return cos(1.5707963267948966);
}

// run: test_cos_pi_half() ~= 0.0 (tolerance: 0.01)

float test_cos_pi() {
    // cos(π) should be -1
    return cos(3.141592653589793);
}

// run: test_cos_pi() ~= -1.0

float test_cos_three_pi_half() {
    // cos(3π/2) should be 0
    return cos(4.71238898038469);
}

// run: test_cos_three_pi_half() ~= 0.0 (tolerance: 0.01)

float test_cos_two_pi() {
    // cos(2π) should be 1
    return cos(6.283185307179586);
}

// run: test_cos_two_pi() ~= 1.0

float test_cos_negative() {
    // cos(-π/2) should be 0
    return cos(-1.5707963267948966);
}

// run: test_cos_negative() ~= 0.0 (tolerance: 0.01)

float test_cos_fraction() {
    // cos(π/4) should be √2/2 ≈ 0.7071067811865476
    return cos(0.7853981633974483);
}

// run: test_cos_fraction() ~= 0.7071067811865476

vec2 test_cos_vec2() {
    // Test with vec2
    return cos(vec2(0.0, 1.5707963267948966));
}

// run: test_cos_vec2() ~= vec2(1.0, 0.0) (tolerance: 0.01)

vec3 test_cos_vec3() {
    // Test with vec3
    return cos(vec3(0.0, 1.5707963267948966, 3.141592653589793));
}

// run: test_cos_vec3() ~= vec3(1.0, 0.0, -1.0) (tolerance: 0.01)

vec4 test_cos_vec4() {
    // Test with vec4
    return cos(vec4(0.0, 1.5707963267948966, 3.141592653589793, 4.71238898038469));
}

// run: test_cos_vec4() ~= vec4(1.0, 0.0, -1.0, 0.0) (tolerance: 0.01)



