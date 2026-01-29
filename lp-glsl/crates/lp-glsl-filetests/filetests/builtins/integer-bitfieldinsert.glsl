// test run
// target riscv32.q32

// ============================================================================
// bitfieldInsert(): Bitfield insert function
// bitfieldInsert(base, insert, offset, bits) - insert bitfield
// Inserts bits [0, bits-1] of insert into base at [offset, offset+bits-1]
// Undefined if offset < 0, bits < 0, or offset + bits > 32
// ============================================================================

int test_bitfieldinsert_int_simple() {
    // bitfieldInsert(0b00001111, 0b1010, 4, 4) should insert 1010 at bits 4-7: 0b10101111 = 175
    return bitfieldInsert(15, 10, 4, 4);
}

// run: test_bitfieldinsert_int_simple() == 175 [expect-fail]

int test_bitfieldinsert_int_lsb() {
    // bitfieldInsert(0b11110000, 0b0011, 0, 4) should insert 0011 at bits 0-3: 0b11110011 = 243
    return bitfieldInsert(240, 3, 0, 4);
}

// run: test_bitfieldinsert_int_lsb() == 243 [expect-fail]

int test_bitfieldinsert_int_msb() {
    // bitfieldInsert(0b00001111, 0b1100, 4, 4) should insert 1100 at bits 4-7: 0b11001111 = 207
    return bitfieldInsert(15, 12, 4, 4);
}

// run: test_bitfieldinsert_int_msb() == 207 [expect-fail]

uint test_bitfieldinsert_uint_simple() {
    // bitfieldInsert(uint(15), uint(10), 4, 4) should insert 1010 at bits 4-7: 175
    return bitfieldInsert(15u, 10u, 4, 4);
}

// run: test_bitfieldinsert_uint_simple() == 175u [expect-fail]

uint test_bitfieldinsert_uint_single_bit() {
    // bitfieldInsert(uint(0), uint(1), 2, 1) should set bit 2: 4
    return bitfieldInsert(0u, 1u, 2, 1);
}

// run: test_bitfieldinsert_uint_single_bit() == 4u [expect-fail]

ivec2 test_bitfieldinsert_ivec2() {
    // bitfieldInsert with ivec2
    return bitfieldInsert(ivec2(15, 240), ivec2(10, 3), 4, 4);
}

// run: test_bitfieldinsert_ivec2() == ivec2(175, 243) [expect-fail]




