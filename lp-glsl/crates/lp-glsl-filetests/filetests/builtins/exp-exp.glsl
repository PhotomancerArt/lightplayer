// test run
// target riscv32.q32

// ============================================================================
// exp(): Natural exponential function
// exp(x) returns e^x
// ============================================================================

float test_exp_zero() {
    // exp(0) should be 1
    return exp(0.0);
}

// run: test_exp_zero() ~= 1.0

float test_exp_one() {
    // exp(1) should be e ≈ 2.718281828459045
    return exp(1.0);
}

// run: test_exp_one() ~= 2.718281828459045

float test_exp_neg_one() {
    // exp(-1) should be 1/e ≈ 0.36787944117144233
    return exp(-1.0);
}

// run: test_exp_neg_one() ~= 0.36787944117144233

float test_exp_two() {
    // exp(2) should be e^2 ≈ 7.38905609893065
    return exp(2.0);
}

// run: test_exp_two() ~= 7.38905609893065 (tolerance: 0.001)

float test_exp_neg_two() {
    // exp(-2) should be e^-2 ≈ 0.1353352832366127
    return exp(-2.0);
}

// run: test_exp_neg_two() ~= 0.1353352832366127

float test_exp_half() {
    // exp(0.5) should be √e ≈ 1.6487212711532444
    return exp(0.5);
}

// run: test_exp_half() ~= 1.6487212711532444

vec2 test_exp_vec2() {
    // Test with vec2
    return exp(vec2(0.0, 1.0));
}

// run: test_exp_vec2() ~= vec2(1.0, 2.718281828459045)

vec3 test_exp_vec3() {
    // Test with vec3
    return exp(vec3(0.0, 1.0, -1.0));
}

// run: test_exp_vec3() ~= vec3(1.0, 2.718281828459045, 0.36787944117144233)

vec4 test_exp_vec4() {
    // Test with vec4
    return exp(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_exp_vec4() ~= vec4(1.0, 1.6487212711532444, 2.718281828459045, 0.6065306597126334)



