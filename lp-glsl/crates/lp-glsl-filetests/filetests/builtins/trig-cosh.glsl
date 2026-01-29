// test run
// target riscv32.q32

// ============================================================================
// cosh(): Hyperbolic cosine function
// cosh(x) = (e^x + e^-x) / 2
// ============================================================================

float test_cosh_zero() {
    // cosh(0) should be 1
    return cosh(0.0);
}

// run: test_cosh_zero() ~= 1.0

float test_cosh_one() {
    // cosh(1) should be approximately 1.5430806348152437
    return cosh(1.0);
}

// run: test_cosh_one() ~= 1.5430806348152437

float test_cosh_neg_one() {
    // cosh(-1) should be approximately 1.5430806348152437
    return cosh(-1.0);
}

// run: test_cosh_neg_one() ~= 1.5430806348152437

float test_cosh_two() {
    // cosh(2) should be approximately 3.7621956910836314
    return cosh(2.0);
}

// run: test_cosh_two() ~= 3.7621956910836314

float test_cosh_neg_two() {
    // cosh(-2) should be approximately 3.7621956910836314
    return cosh(-2.0);
}

// run: test_cosh_neg_two() ~= 3.7621956910836314

float test_cosh_half() {
    // cosh(0.5) should be approximately 1.1276259652063807
    return cosh(0.5);
}

// run: test_cosh_half() ~= 1.1276259652063807

vec2 test_cosh_vec2() {
    // Test with vec2
    return cosh(vec2(0.0, 1.0));
}

// run: test_cosh_vec2() ~= vec2(1.0, 1.5430806348152437)

vec3 test_cosh_vec3() {
    // Test with vec3
    return cosh(vec3(0.0, 1.0, -1.0));
}

// run: test_cosh_vec3() ~= vec3(1.0, 1.5430806348152437, 1.5430806348152437)

vec4 test_cosh_vec4() {
    // Test with vec4
    return cosh(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_cosh_vec4() ~= vec4(1.0, 1.1276259652063807, 1.5430806348152437, 1.1276259652063807)




