// test run
// target riscv32.q32

// ============================================================================
// log2(): Base 2 logarithm function
// log2(x) returns log2(x)
// Undefined if x <= 0
// ============================================================================

float test_log2_one() {
    // log2(1) should be 0
    return log2(1.0);
}

// run: test_log2_one() ~= 0.0

float test_log2_two() {
    // log2(2) should be 1
    return log2(2.0);
}

// run: test_log2_two() ~= 1.0

float test_log2_four() {
    // log2(4) should be 2
    return log2(4.0);
}

// run: test_log2_four() ~= 2.0

float test_log2_eight() {
    // log2(8) should be 3
    return log2(8.0);
}

// run: test_log2_eight() ~= 3.0

float test_log2_half() {
    // log2(0.5) should be -1
    return log2(0.5);
}

// run: test_log2_half() ~= -1.0

float test_log2_sqrt_two() {
    // log2(√2) should be 0.5
    return log2(1.4142135623730951);
}

// run: test_log2_sqrt_two() ~= 0.5

vec2 test_log2_vec2() {
    // Test with vec2
    return log2(vec2(1.0, 2.0));
}

// run: test_log2_vec2() ~= vec2(0.0, 1.0)

vec3 test_log2_vec3() {
    // Test with vec3
    return log2(vec3(1.0, 2.0, 4.0));
}

// run: test_log2_vec3() ~= vec3(0.0, 1.0, 2.0)

vec4 test_log2_vec4() {
    // Test with vec4
    return log2(vec4(1.0, 2.0, 0.5, 0.25));
}

// run: test_log2_vec4() ~= vec4(0.0, 1.0, -1.0, -2.0)




