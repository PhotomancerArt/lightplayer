// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// acosh(): Arc hyperbolic cosine function
// Inverse of cosh, undefined if x < 1
// ============================================================================

float test_acosh_one() {
    // acosh(1) should be 0
    return acosh(rt(1.0));
}

// run: test_acosh_one() ~= 0.0

float test_acosh_cosh_one() {
    // acosh(cosh(1)) should be approximately 1
    return acosh(rt(cosh(1.0)));
}

// run: test_acosh_cosh_one() ~= 1.0

float test_acosh_two() {
    // acosh(2) should be approximately 1.3169578969248166
    return acosh(rt(2.0));
}

// run: test_acosh_two() ~= 1.3169578969248166

float test_acosh_five() {
    // acosh(5) should be approximately 2.2924316695611777
    return acosh(rt(5.0));
}

// run: test_acosh_five() ~= 2.2924316695611777

float test_acosh_large() {
    // acosh(10) should be approximately 2.993222846126381
    return acosh(rt(10.0));
}

// run: test_acosh_large() ~= 2.993222846126381

vec2 test_acosh_vec2() {
    // Test with vec2
    return acosh(vec2(rt(1.0), rt(2.0)));
}

// @broken(rv32n.q32)
// run: test_acosh_vec2() ~= vec2(0.0, 1.3169578969248166)

vec3 test_acosh_vec3() {
    // Test with vec3
    return acosh(vec3(rt(1.0), rt(2.0), rt(5.0)));
}

// run: test_acosh_vec3() ~= vec3(0.0, 1.3169578969248166, 2.2924316695611777)

vec4 test_acosh_vec4() {
    // Test with vec4
    return acosh(vec4(rt(1.0), rt(1.5), rt(2.0), rt(3.0)));
}

// run: test_acosh_vec4() ~= vec4(0.0, 0.9624236501192069, 1.3169578969248166, 1.762747174039086)




