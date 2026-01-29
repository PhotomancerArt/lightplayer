// test run
// target riscv32.q32

// ============================================================================
// ldexp(): Scale by power of 2 function
// ldexp(x, exp) returns x * 2^exp
// ============================================================================

float test_ldexp_one_zero() {
    // ldexp(1.0, 0) should be 1.0 * 2^0 = 1.0
    return ldexp(1.0, 0);
}

// run: test_ldexp_one_zero() ~= 1.0 [expect-fail]

float test_ldexp_one_one() {
    // ldexp(1.0, 1) should be 1.0 * 2^1 = 2.0
    return ldexp(1.0, 1);
}

// run: test_ldexp_one_one() ~= 2.0 [expect-fail]

float test_ldexp_one_two() {
    // ldexp(1.0, 2) should be 1.0 * 2^2 = 4.0
    return ldexp(1.0, 2);
}

// run: test_ldexp_one_two() ~= 4.0 [expect-fail]

float test_ldexp_half_neg_one() {
    // ldexp(0.5, -1) should be 0.5 * 2^(-1) = 0.25
    return ldexp(0.5, -1);
}

// run: test_ldexp_half_neg_one() ~= 0.25 [expect-fail]

float test_ldexp_two_neg_one() {
    // ldexp(2.0, -1) should be 2.0 * 2^(-1) = 1.0
    return ldexp(2.0, -1);
}

// run: test_ldexp_two_neg_one() ~= 1.0 [expect-fail]

float test_ldexp_three_one() {
    // ldexp(3.0, 1) should be 3.0 * 2^1 = 6.0
    return ldexp(3.0, 1);
}

// run: test_ldexp_three_one() ~= 6.0 [expect-fail]

vec2 test_ldexp_vec2() {
    // Test with vec2
    return ldexp(vec2(1.0, 0.5), ivec2(1, -1));
}

// run: test_ldexp_vec2() ~= vec2(2.0, 0.25) [expect-fail]

vec3 test_ldexp_vec3() {
    // Test with vec3
    return ldexp(vec3(1.0, 2.0, 3.0), ivec3(0, 1, 2));
}

// run: test_ldexp_vec3() ~= vec3(1.0, 4.0, 12.0) [expect-fail]

vec4 test_ldexp_vec4() {
    // Test with vec4
    return ldexp(vec4(1.0, 0.5, 2.0, 3.0), ivec4(1, -1, 0, 1));
}

// run: test_ldexp_vec4() ~= vec4(2.0, 0.25, 2.0, 6.0) [expect-fail]




