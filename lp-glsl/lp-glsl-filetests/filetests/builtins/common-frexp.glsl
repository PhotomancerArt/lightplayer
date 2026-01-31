// test run
// target riscv32.q32

// ============================================================================
// frexp(): Extract mantissa and exponent function
// frexp(x, out exp) returns mantissa, sets exp to exponent
// x = mantissa * 2^exp, where 0.5 <= |mantissa| < 1.0
// ============================================================================

vec2 test_frexp_one() {
    // frexp(1.0) should return (0.5, 1) since 1.0 = 0.5 * 2^1
    int exp;
    float mant = frexp(1.0, exp);
    return vec2(mant, float(exp));
}

// run: test_frexp_one() ~= vec2(0.5, 1.0) [expect-fail]

vec2 test_frexp_two() {
    // frexp(2.0) should return (0.5, 2) since 2.0 = 0.5 * 2^2
    int exp;
    float mant = frexp(2.0, exp);
    return vec2(mant, float(exp));
}

// run: test_frexp_two() ~= vec2(0.5, 2.0) [expect-fail]

vec2 test_frexp_half() {
    // frexp(0.5) should return (0.5, 0) since 0.5 = 0.5 * 2^0
    int exp;
    float mant = frexp(0.5, exp);
    return vec2(mant, float(exp));
}

// run: test_frexp_half() ~= vec2(0.5, 0.0) [expect-fail]

vec2 test_frexp_four() {
    // frexp(4.0) should return (0.5, 3) since 4.0 = 0.5 * 2^3
    int exp;
    float mant = frexp(4.0, exp);
    return vec2(mant, float(exp));
}

// run: test_frexp_four() ~= vec2(0.5, 3.0) [expect-fail]

vec2 test_frexp_eight() {
    // frexp(8.0) should return (0.5, 4) since 8.0 = 0.5 * 2^4
    int exp;
    float mant = frexp(8.0, exp);
    return vec2(mant, float(exp));
}

// run: test_frexp_eight() ~= vec2(0.5, 4.0) [expect-fail]

vec4 test_frexp_vec2() {
    // Test with vec2
    ivec2 exp;
    vec2 mant = frexp(vec2(1.0, 2.0), exp);
    return vec4(mant.x, mant.y, float(exp.x), float(exp.y));
}

// run: test_frexp_vec2() ~= vec4(0.5, 0.5, 1.0, 2.0) [expect-fail]




