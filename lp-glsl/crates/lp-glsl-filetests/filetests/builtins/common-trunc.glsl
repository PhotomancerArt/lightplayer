// test run
// target riscv32.q32

// ============================================================================
// trunc(): Truncate toward zero function
// trunc(x) returns the integer part of x (toward zero)
// ============================================================================

float test_trunc_integer() {
    // trunc(5.0) should be 5.0
    return trunc(5.0);
}

// run: test_trunc_integer() ~= 5.0 [expect-fail]

float test_trunc_positive() {
    // trunc(3.7) should be 3.0
    return trunc(3.7);
}

// run: test_trunc_positive() ~= 3.0 [expect-fail]

float test_trunc_negative() {
    // trunc(-2.3) should be -2.0
    return trunc(-2.3);
}

// run: test_trunc_negative() ~= -2.0 [expect-fail]

float test_trunc_negative_small() {
    // trunc(-0.1) should be 0.0
    return trunc(-0.1);
}

// run: test_trunc_negative_small() ~= 0.0 [expect-fail]

float test_trunc_half() {
    // trunc(2.5) should be 2.0
    return trunc(2.5);
}

// run: test_trunc_half() ~= 2.0 [expect-fail]

vec2 test_trunc_vec2() {
    // Test with vec2
    return trunc(vec2(1.9, -2.1));
}

// run: test_trunc_vec2() ~= vec2(1.0, -2.0) [expect-fail]

vec3 test_trunc_vec3() {
    // Test with vec3
    return trunc(vec3(0.0, 3.7, -1.5));
}

// run: test_trunc_vec3() ~= vec3(0.0, 3.0, -1.0) [expect-fail]

vec4 test_trunc_vec4() {
    // Test with vec4
    return trunc(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_trunc_vec4() ~= vec4(1.0, 2.0, 0.0, 4.0) [expect-fail]




