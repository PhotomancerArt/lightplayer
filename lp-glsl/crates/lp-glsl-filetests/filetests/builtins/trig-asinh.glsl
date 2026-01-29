// test run
// target riscv32.q32

// ============================================================================
// asinh(): Arc hyperbolic sine function
// Inverse of sinh
// ============================================================================

float test_asinh_zero() {
    // asinh(0) should be 0
    return asinh(0.0);
}

// run: test_asinh_zero() ~= 0.0

float test_asinh_one() {
    // asinh(1) should be approximately 0.881373587019543
    return asinh(1.0);
}

// run: test_asinh_one() ~= 0.881373587019543

float test_asinh_neg_one() {
    // asinh(-1) should be approximately -0.881373587019543
    return asinh(-1.0);
}

// run: test_asinh_neg_one() ~= -0.881373587019543

float test_asinh_two() {
    // asinh(2) should be approximately 1.4436354751788103
    return asinh(2.0);
}

// run: test_asinh_two() ~= 1.4436354751788103

float test_asinh_neg_two() {
    // asinh(-2) should be approximately -1.4436354751788103
    return asinh(-2.0);
}

// run: test_asinh_neg_two() ~= -1.4436354751788103

float test_asinh_sinh_one() {
    // asinh(sinh(1)) should be approximately 1
    return asinh(sinh(1.0));
}

// run: test_asinh_sinh_one() ~= 1.0

vec2 test_asinh_vec2() {
    // Test with vec2
    return asinh(vec2(0.0, 1.0));
}

// run: test_asinh_vec2() ~= vec2(0.0, 0.881373587019543)

vec3 test_asinh_vec3() {
    // Test with vec3
    return asinh(vec3(0.0, 1.0, -1.0));
}

// run: test_asinh_vec3() ~= vec3(0.0, 0.881373587019543, -0.881373587019543)

vec4 test_asinh_vec4() {
    // Test with vec4
    return asinh(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_asinh_vec4() ~= vec4(0.0, 0.48121182505960347, 0.881373587019543, -0.48121182505960347)




