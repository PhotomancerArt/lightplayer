// test run
// target riscv32.q32

// ============================================================================
// intBitsToFloat(): Int bits to float function
// intBitsToFloat(x) bit casts int to float
// ============================================================================

float test_intbitstofloat_zero() {
    // intBitsToFloat(0) should be 0.0
    return intBitsToFloat(0);
}

// run: test_intbitstofloat_zero() ~= 0.0 [expect-fail]

float test_intbitstofloat_one() {
    // intBitsToFloat bit representation of 1.0 should be 1.0
    return intBitsToFloat(1065353216);
}

// run: test_intbitstofloat_one() ~= 1.0 [expect-fail]

float test_intbitstofloat_neg_one() {
    // intBitsToFloat bit representation of -1.0 should be -1.0
    return intBitsToFloat(-1082130432);
}

// run: test_intbitstofloat_neg_one() ~= -1.0 [expect-fail]

float test_intbitstofloat_inf() {
    // intBitsToFloat bit representation of positive infinity
    return intBitsToFloat(2139095040);
}

// run: test_intbitstofloat_inf() ~= 1.0 / 0.0 [expect-fail]

float test_intbitstofloat_neg_inf() {
    // intBitsToFloat bit representation of negative infinity
    return intBitsToFloat(-8388608);
}

// run: test_intbitstofloat_neg_inf() ~= -1.0 / 0.0 [expect-fail]

vec2 test_intbitstofloat_vec2() {
    // Test with ivec2
    return intBitsToFloat(ivec2(1065353216, -1082130432));
}

// run: test_intbitstofloat_vec2() ~= vec2(1.0, -1.0) [expect-fail]

vec3 test_intbitstofloat_vec3() {
    // Test with ivec3
    return intBitsToFloat(ivec3(0, 1065353216, 1073741824));
}

// run: test_intbitstofloat_vec3() ~= vec3(0.0, 1.0, 2.0) [expect-fail]

vec4 test_intbitstofloat_vec4() {
    // Test with ivec4
    return intBitsToFloat(ivec4(1065353216, 0, -1082130432, 2139095040));
}

// run: test_intbitstofloat_vec4() ~= vec4(1.0, 0.0, -1.0, 1.0 / 0.0) [expect-fail]




