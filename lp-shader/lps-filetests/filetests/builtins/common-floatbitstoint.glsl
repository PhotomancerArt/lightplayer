// test run

// ============================================================================
// floatBitsToInt(): Float bits to int function
// floatBitsToInt(x) bit casts float to int
// ============================================================================

int test_floatbitstoint_zero() {
    // floatBitsToInt(0.0) should be 0
    return floatBitsToInt(0.0);
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_zero() == 0

int test_floatbitstoint_one() {
    // floatBitsToInt(1.0) should be the bit representation of 1.0
    return floatBitsToInt(1.0);
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_one() == 1065353216

int test_floatbitstoint_neg_one() {
    // floatBitsToInt(-1.0) should be the bit representation of -1.0
    return floatBitsToInt(-1.0);
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_neg_one() == -1082130432

int test_floatbitstoint_inf() {
    // floatBitsToInt with positive infinity
    return floatBitsToInt(1.0 / 0.0);
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_inf() == 2139095040

int test_floatbitstoint_neg_inf() {
    // floatBitsToInt with negative infinity
    return floatBitsToInt(-1.0 / 0.0);
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_neg_inf() == -8388608

ivec2 test_floatbitstoint_vec2() {
    // Test with vec2
    return floatBitsToInt(vec2(1.0, -1.0));
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_vec2() == ivec2(1065353216, -1082130432)

ivec3 test_floatbitstoint_vec3() {
    // Test with vec3
    return floatBitsToInt(vec3(0.0, 1.0, 2.0));
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_vec3() == ivec3(0, 1065353216, 1073741824)

ivec4 test_floatbitstoint_vec4() {
    // Test with vec4
    return floatBitsToInt(vec4(1.0, 0.0, -1.0, 1.0 / 0.0));
}

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(rv32n.q32)
// @unsupported(jit.q32)
// run: test_floatbitstoint_vec4() == ivec4(1065353216, 0, -1082130432, 2139095040)




