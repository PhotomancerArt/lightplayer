// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// atanh(): Arc hyperbolic tangent function
// Inverse of tanh, undefined if |x| >= 1
// ============================================================================

float test_atanh_zero() {
    // atanh(0) should be 0
    return atanh(rt(0.0));
}

// run: test_atanh_zero() ~= 0.0

float test_atanh_half() {
    // atanh(0.5) should be approximately 0.5493061443340548
    return atanh(rt(0.5));
}

// run: test_atanh_half() ~= 0.5493061443340548

float test_atanh_neg_half() {
    // atanh(-0.5) should be approximately -0.5493061443340548
    return atanh(rt(-0.5));
}

// run: test_atanh_neg_half() ~= -0.5493061443340548

float test_atanh_tanh_half() {
    // atanh(tanh(0.5)) should be approximately 0.5
    return atanh(rt(tanh(0.5)));
}

// run: test_atanh_tanh_half() ~= 0.5

float test_atanh_small() {
    // atanh(0.1) should be approximately 0.10033534773107558
    return atanh(rt(0.1));
}

// run: test_atanh_small() ~= 0.10033534773107558

float test_atanh_neg_small() {
    // atanh(-0.1) should be approximately -0.10033534773107558
    return atanh(rt(-0.1));
}

// run: test_atanh_neg_small() ~= -0.10033534773107558

vec2 test_atanh_vec2() {
    // Test with vec2
    return atanh(vec2(rt(0.0), rt(0.5)));
}

// @broken(rv32n.q32)
// run: test_atanh_vec2() ~= vec2(0.0, 0.5493061443340548)

vec3 test_atanh_vec3() {
    // Test with vec3
    return atanh(vec3(rt(0.0), rt(0.5), rt(-0.5)));
}

// run: test_atanh_vec3() ~= vec3(0.0, 0.5493061443340548, -0.5493061443340548)

vec4 test_atanh_vec4() {
    // Test with vec4
    return atanh(vec4(rt(0.0), rt(0.1), rt(0.5), rt(-0.1)));
}

// run: test_atanh_vec4() ~= vec4(0.0, 0.10033534773107558, 0.5493061443340548, -0.10033534773107558)




