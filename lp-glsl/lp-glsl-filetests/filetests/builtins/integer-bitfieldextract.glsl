// test run
// target riscv32.q32

// ============================================================================
// bitfieldExtract(): Bitfield extract function
// bitfieldExtract(value, offset, bits) - extract bitfield
// Extracts bits [offset, offset + bits - 1]
// Undefined if offset < 0, bits < 0, or offset + bits > 32
// ============================================================================

int test_bitfieldextract_int_simple() {
    // bitfieldExtract(0b11110000, 4, 4) should extract bits 4-7: 0b1111 = 15
    return bitfieldExtract(240, 4, 4);
}

// run: test_bitfieldextract_int_simple() == 15 [expect-fail]

int test_bitfieldextract_int_lsb() {
    // bitfieldExtract(0b10101010, 0, 4) should extract bits 0-3: 0b1010 = 10
    return bitfieldExtract(170, 0, 4);
}

// run: test_bitfieldextract_int_lsb() == 10 [expect-fail]

int test_bitfieldextract_int_msb() {
    // bitfieldExtract(0b10101010, 4, 4) should extract bits 4-7: 0b1010 = 10
    return bitfieldExtract(170, 4, 4);
}

// run: test_bitfieldextract_int_msb() == 10 [expect-fail]

uint test_bitfieldextract_uint_simple() {
    // bitfieldExtract(uint(240), 4, 4) should extract bits 4-7: 0b1111 = 15
    return bitfieldExtract(240u, 4, 4);
}

// run: test_bitfieldextract_uint_simple() == 15u [expect-fail]

uint test_bitfieldextract_uint_single_bit() {
    // bitfieldExtract(uint(0b0100), 2, 1) should extract bit 2: 1
    return bitfieldExtract(4u, 2, 1);
}

// run: test_bitfieldextract_uint_single_bit() == 1u [expect-fail]

ivec2 test_bitfieldextract_ivec2() {
    // bitfieldExtract with ivec2
    return bitfieldExtract(ivec2(240, 170), 4, 4);
}

// run: test_bitfieldextract_ivec2() == ivec2(15, 10) [expect-fail]




