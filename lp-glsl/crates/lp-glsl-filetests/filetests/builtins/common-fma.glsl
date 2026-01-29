// test run
// target riscv32.q32

// ============================================================================
// fma(): Fused multiply-add function
// fma(a, b, c) returns a * b + c with higher precision
// ============================================================================

float test_fma_simple() {
    // fma(2.0, 3.0, 4.0) should be 2*3+4 = 10
    return fma(2.0, 3.0, 4.0);
}

// run: test_fma_simple() ~= 10.0 [expect-fail]

float test_fma_zero_c() {
    // fma(2.0, 3.0, 0.0) should be 2*3+0 = 6
    return fma(2.0, 3.0, 0.0);
}

// run: test_fma_zero_c() ~= 6.0 [expect-fail]

float test_fma_negative() {
    // fma(2.0, -3.0, 5.0) should be 2*(-3)+5 = -1
    return fma(2.0, -3.0, 5.0);
}

// run: test_fma_negative() ~= -1.0 [expect-fail]

float test_fma_fractions() {
    // fma(1.5, 2.0, 0.5) should be 1.5*2.0+0.5 = 3.5
    return fma(1.5, 2.0, 0.5);
}

// run: test_fma_fractions() ~= 3.5 [expect-fail]

float test_fma_one_one_one() {
    // fma(1.0, 1.0, 1.0) should be 1*1+1 = 2
    return fma(1.0, 1.0, 1.0);
}

// run: test_fma_one_one_one() ~= 2.0 [expect-fail]

vec2 test_fma_vec2() {
    // Test with vec2
    return fma(vec2(2.0, 1.5), vec2(3.0, 2.0), vec2(4.0, 0.5));
}

// run: test_fma_vec2() ~= vec2(10.0, 3.5) [expect-fail]

vec3 test_fma_vec3() {
    // Test with vec3
    return fma(vec3(2.0, 1.0, 1.5), vec3(-3.0, 1.0, 2.0), vec3(5.0, 1.0, 0.5));
}

// run: test_fma_vec3() ~= vec3(-1.0, 2.0, 3.5) [expect-fail]

vec4 test_fma_vec4() {
    // Test with vec4
    return fma(vec4(1.0, 2.0, 1.5, 0.5), vec4(1.0, 3.0, 2.0, 4.0), vec4(1.0, 4.0, 0.5, 2.0));
}

// run: test_fma_vec4() ~= vec4(2.0, 10.0, 3.5, 4.0) [expect-fail]




