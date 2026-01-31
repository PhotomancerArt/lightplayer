// test run
// target riscv32.q32

// ============================================================================
// unpackUnorm*(): Unpack normalized values functions
// unpackUnorm2x16(uint) - unpack uint to vec2
// unpackUnorm4x8(uint) - unpack uint to vec4
// ============================================================================

vec2 test_unpackunorm2x16_zero() {
    // unpackUnorm2x16(0) should unpack to vec2(0.0, 0.0)
    return unpackUnorm2x16(0u);
}

// run: test_unpackunorm2x16_zero() ~= vec2(0.0, 0.0) [expect-fail]

vec2 test_unpackunorm2x16_max() {
    // unpackUnorm2x16(all bits set) should unpack to vec2(1.0, 1.0)
    return unpackUnorm2x16(4294967295u);
}

// run: test_unpackunorm2x16_max() ~= vec2(1.0, 1.0) [expect-fail]

vec2 test_unpackunorm2x16_half() {
    // unpackUnorm2x16 half value
    return unpackUnorm2x16(2147516416u);
}

// run: test_unpackunorm2x16_half() ~= vec2(0.5, 0.5) [expect-fail]

vec4 test_unpackunorm4x8_zero() {
    // unpackUnorm4x8(0) should unpack to vec4(0.0, 0.0, 0.0, 0.0)
    return unpackUnorm4x8(0u);
}

// run: test_unpackunorm4x8_zero() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

vec4 test_unpackunorm4x8_max() {
    // unpackUnorm4x8(all bits set) should unpack to vec4(1.0, 1.0, 1.0, 1.0)
    return unpackUnorm4x8(4294967295u);
}

// run: test_unpackunorm4x8_max() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

vec4 test_unpackunorm4x8_quarters() {
    // unpackUnorm4x8 quarter values
    return unpackUnorm4x8(67372036u);
}

// run: test_unpackunorm4x8_quarters() ~= vec4(0.25, 0.25, 0.25, 0.25) [expect-fail]




