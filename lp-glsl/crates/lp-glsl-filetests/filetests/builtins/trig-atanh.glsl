// test run
// target riscv32.q32

// ============================================================================
// atanh(): Arc hyperbolic tangent function
// Inverse of tanh, undefined if |x| >= 1
// ============================================================================

float test_atanh_zero() {
    // atanh(0) should be 0
    return atanh(0.0);
}

// run: test_atanh_zero() ~= 0.0

float test_atanh_half() {
    // atanh(0.5) should be approximately 0.5493061443340548
    return atanh(0.5);
}

// run: test_atanh_half() ~= 0.5493061443340548

float test_atanh_neg_half() {
    // atanh(-0.5) should be approximately -0.5493061443340548
    return atanh(-0.5);
}

// run: test_atanh_neg_half() ~= -0.5493061443340548

float test_atanh_tanh_half() {
    // atanh(tanh(0.5)) should be approximately 0.5
    return atanh(tanh(0.5));
}

// run: test_atanh_tanh_half() ~= 0.5

float test_atanh_small() {
    // atanh(0.1) should be approximately 0.10033534773107558
    return atanh(0.1);
}

// run: test_atanh_small() ~= 0.10033534773107558

float test_atanh_neg_small() {
    // atanh(-0.1) should be approximately -0.10033534773107558
    return atanh(-0.1);
}

// run: test_atanh_neg_small() ~= -0.10033534773107558

vec2 test_atanh_vec2() {
    // Test with vec2
    return atanh(vec2(0.0, 0.5));
}

// run: test_atanh_vec2() ~= vec2(0.0, 0.5493061443340548)

vec3 test_atanh_vec3() {
    // Test with vec3
    return atanh(vec3(0.0, 0.5, -0.5));
}

// run: test_atanh_vec3() ~= vec3(0.0, 0.5493061443340548, -0.5493061443340548)

vec4 test_atanh_vec4() {
    // Test with vec4
    return atanh(vec4(0.0, 0.1, 0.5, -0.1));
}

// run: test_atanh_vec4() ~= vec4(0.0, 0.10033534773107558, 0.5493061443340548, -0.10033534773107558)




