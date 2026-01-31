// test run
// target riscv32.q32

// ============================================================================
// bitfieldReverse(): Bitfield reverse function
// bitfieldReverse(value) - reverses bits of value
// ============================================================================

int test_bitfieldreverse_int_simple() {
    // bitfieldReverse(0b00000001) should be 0b10000000 = -2147483648 (sign bit)
    return bitfieldReverse(1);
}

// run: test_bitfieldreverse_int_simple() == -2147483648 [expect-fail]

int test_bitfieldreverse_int_pattern() {
    // bitfieldReverse(0b10101010) should reverse the bits
    return bitfieldReverse(170);
}

// run: test_bitfieldreverse_int_pattern() == 1431655765 [expect-fail]

int test_bitfieldreverse_int_zero() {
    // bitfieldReverse(0) should be 0
    return bitfieldReverse(0);
}

// run: test_bitfieldreverse_int_zero() == 0 [expect-fail]

uint test_bitfieldreverse_uint_simple() {
    // bitfieldReverse(uint(1)) should be 0b10000000...0 = 2147483648
    return bitfieldReverse(1u);
}

// run: test_bitfieldreverse_uint_simple() == 2147483648u [expect-fail]

uint test_bitfieldreverse_uint_pattern() {
    // bitfieldReverse(uint(170)) should reverse the bits
    return bitfieldReverse(170u);
}

// run: test_bitfieldreverse_uint_pattern() == 1431655765u [expect-fail]

uint test_bitfieldreverse_uint_max() {
    // bitfieldReverse(uint max) should be uint max
    return bitfieldReverse(4294967295u);
}

// run: test_bitfieldreverse_uint_max() == 4294967295u [expect-fail]

ivec2 test_bitfieldreverse_ivec2() {
    // bitfieldReverse with ivec2
    return bitfieldReverse(ivec2(1, 170));
}

// run: test_bitfieldreverse_ivec2() == ivec2(-2147483648, 1431655765) [expect-fail]




