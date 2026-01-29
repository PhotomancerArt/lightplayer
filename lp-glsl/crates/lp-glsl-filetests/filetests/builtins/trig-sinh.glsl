// test run
// target riscv32.q32

// ============================================================================
// sinh(): Hyperbolic sine function
// sinh(x) = (e^x - e^-x) / 2
// ============================================================================

float test_sinh_zero() {
    // sinh(0) should be 0
    return sinh(0.0);
}

// run: test_sinh_zero() ~= 0.0

float test_sinh_one() {
    // sinh(1) should be approximately 1.1752011936438014
    return sinh(1.0);
}

// run: test_sinh_one() ~= 1.1752011936438014

float test_sinh_neg_one() {
    // sinh(-1) should be approximately -1.1752011936438014
    return sinh(-1.0);
}

// run: test_sinh_neg_one() ~= -1.1752011936438014

float test_sinh_two() {
    // sinh(2) should be approximately 3.626860407847019
    return sinh(2.0);
}

// run: test_sinh_two() ~= 3.626860407847019

float test_sinh_neg_two() {
    // sinh(-2) should be approximately -3.626860407847019
    return sinh(-2.0);
}

// run: test_sinh_neg_two() ~= -3.626860407847019

float test_sinh_half() {
    // sinh(0.5) should be approximately 0.5210953054937474
    return sinh(0.5);
}

// run: test_sinh_half() ~= 0.5210953054937474

vec2 test_sinh_vec2() {
    // Test with vec2
    return sinh(vec2(0.0, 1.0));
}

// run: test_sinh_vec2() ~= vec2(0.0, 1.1752011936438014)

vec3 test_sinh_vec3() {
    // Test with vec3
    return sinh(vec3(0.0, 1.0, -1.0));
}

// run: test_sinh_vec3() ~= vec3(0.0, 1.1752011936438014, -1.1752011936438014)

vec4 test_sinh_vec4() {
    // Test with vec4
    return sinh(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_sinh_vec4() ~= vec4(0.0, 0.5210953054937474, 1.1752011936438014, -0.5210953054937474)




