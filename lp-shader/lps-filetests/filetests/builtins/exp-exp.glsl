// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// exp(): Natural exponential function
// exp(x) returns e^x
// ============================================================================

float test_exp_zero() {
    // exp(0) should be 1
    return exp(rt(0.0));
}

// run: test_exp_zero() ~= 1.0

float test_exp_one() {
    // exp(1) should be e ≈ 2.718281828459045
    return exp(rt(1.0));
}

// run: test_exp_one() ~= 2.718281828459045

float test_exp_neg_one() {
    // exp(-1) should be 1/e ≈ 0.36787944117144233
    return exp(rt(-1.0));
}

// run: test_exp_neg_one() ~= 0.36787944117144233

float test_exp_two() {
    // exp(2) should be e^2 ≈ 7.38905609893065
    return exp(rt(2.0));
}

// run: test_exp_two() ~= 7.38905609893065 (tolerance: 0.001)

float test_exp_neg_two() {
    // exp(-2) should be e^-2 ≈ 0.1353352832366127
    return exp(rt(-2.0));
}

// run: test_exp_neg_two() ~= 0.1353352832366127

float test_exp_half() {
    // exp(0.5) should be √e ≈ 1.6487212711532444
    return exp(rt(0.5));
}

// run: test_exp_half() ~= 1.6487212711532444

vec2 test_exp_vec2() {
    // Test with vec2
    return exp(vec2(rt(0.0), rt(1.0)));
}

// run: test_exp_vec2() ~= vec2(1.0, 2.718281828459045)

vec3 test_exp_vec3() {
    // Test with vec3
    return exp(vec3(rt(0.0), rt(1.0), rt(-1.0)));
}

// run: test_exp_vec3() ~= vec3(1.0, 2.718281828459045, 0.36787944117144233)

vec4 test_exp_vec4() {
    // Test with vec4
    return exp(vec4(rt(0.0), rt(0.5), rt(1.0), rt(-0.5)));
}

// run: test_exp_vec4() ~= vec4(1.0, 1.6487212711532444, 2.718281828459045, 0.6065306597126334)



