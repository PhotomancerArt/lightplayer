// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// exp2(): Base 2 exponential function
// exp2(x) returns 2^x
// ============================================================================

float test_exp2_zero() {
    // exp2(0) should be 1
    return exp2(rt(0.0));
}

// run: test_exp2_zero() ~= 1.0

float test_exp2_one() {
    // exp2(1) should be 2
    return exp2(rt(1.0));
}

// run: test_exp2_one() ~= 2.0

float test_exp2_two() {
    // exp2(2) should be 4
    return exp2(rt(2.0));
}

// run: test_exp2_two() ~= 4.0 (tolerance: 0.001)

float test_exp2_three() {
    // exp2(3) should be 8
    return exp2(rt(3.0));
}

// run: test_exp2_three() ~= 8.0 (tolerance: 0.001)

float test_exp2_neg_one() {
    // exp2(-1) should be 0.5
    return exp2(rt(-1.0));
}

// run: test_exp2_neg_one() ~= 0.5

float test_exp2_half() {
    // exp2(0.5) should be √2 ≈ 1.4142135623730951
    return exp2(rt(0.5));
}

// run: test_exp2_half() ~= 1.4142135623730951

vec2 test_exp2_vec2() {
    // Test with vec2
    return exp2(vec2(rt(0.0), rt(1.0)));
}

// run: test_exp2_vec2() ~= vec2(1.0, 2.0)

vec3 test_exp2_vec3() {
    // Test with vec3
    return exp2(vec3(rt(0.0), rt(1.0), rt(-1.0)));
}

// run: test_exp2_vec3() ~= vec3(1.0, 2.0, 0.5)

vec4 test_exp2_vec4() {
    // Test with vec4
    return exp2(vec4(rt(0.0), rt(0.5), rt(1.0), rt(-0.5)));
}

// run: test_exp2_vec4() ~= vec4(1.0, 1.4142135623730951, 2.0, 0.7071067811865476)



