// test run
// target riscv32.q32

// ============================================================================
// sign(): Sign function
// sign(x) returns 1.0 if x > 0, 0.0 if x = 0, -1.0 if x < 0
// Works with float, int, vec*, ivec*
// ============================================================================

float test_sign_positive() {
    // sign(5.0) should be 1.0
    return sign(5.0);
}

// run: test_sign_positive() ~= 1.0

float test_sign_zero() {
    // sign(0.0) should be 0.0
    return sign(0.0);
}

// run: test_sign_zero() ~= 0.0

float test_sign_negative() {
    // sign(-3.0) should be -1.0
    return sign(-3.0);
}

// run: test_sign_negative() ~= -1.0

vec2 test_sign_vec2() {
    // Test with vec2
    return sign(vec2(2.0, -1.5));
}

// run: test_sign_vec2() ~= vec2(1.0, -1.0)

vec3 test_sign_vec3() {
    // Test with vec3
    return sign(vec3(0.0, 4.0, -2.0));
}

// run: test_sign_vec3() ~= vec3(0.0, 1.0, -1.0)

vec4 test_sign_vec4() {
    // Test with vec4
    return sign(vec4(1.0, 0.0, -1.0, 3.0));
}

// run: test_sign_vec4() ~= vec4(1.0, 0.0, -1.0, 1.0)

int test_sign_int_positive() {
    // sign(5) should be 1
    return sign(5);
}

// run: test_sign_int_positive() == 1

int test_sign_int_zero() {
    // sign(0) should be 0
    return sign(0);
}

// run: test_sign_int_zero() == 0

int test_sign_int_negative() {
    // sign(-3) should be -1
    return sign(-3);
}

// run: test_sign_int_negative() == -1




