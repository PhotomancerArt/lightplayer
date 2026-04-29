// test run

// ============================================================================
// bitCount(): Bit count function
// bitCount(value) - returns number of one bits
// ============================================================================

int test_bitcount_int_zero() {
    // bitCount(0) should be 0
    return bitCount(0);
}

// run: test_bitcount_int_zero() == 0

int test_bitcount_int_one() {
    // bitCount(1) should be 1
    return bitCount(1);
}

// run: test_bitcount_int_one() == 1

int test_bitcount_int_full() {
    // bitCount(-1) should be 32 (all bits set)
    return bitCount(-1);
}

// run: test_bitcount_int_full() == 32

int test_bitcount_int_pattern() {
    // bitCount(0b10101010) should be 4 (4 ones)
    return bitCount(170);
}

// run: test_bitcount_int_pattern() == 4

uint test_bitcount_uint_zero() {
    // bitCount(uint(0)) should be 0
    return bitCount(0u);
}

// @unimplemented(jit.q32)
// run: test_bitcount_uint_zero() == 0u

uint test_bitcount_uint_one() {
    // bitCount(uint(1)) should be 1
    return bitCount(1u);
}

// @unimplemented(jit.q32)
// run: test_bitcount_uint_one() == 1u

uint test_bitcount_uint_full() {
    // bitCount(uint max) should be 32
    return bitCount(4294967295u);
}

// @unimplemented(jit.q32)
// run: test_bitcount_uint_full() == 32u

uint test_bitcount_uint_pattern() {
    // bitCount(uint(170)) should be 4
    return bitCount(170u);
}

// @unimplemented(jit.q32)
// run: test_bitcount_uint_pattern() == 4u

ivec2 test_bitcount_ivec2() {
    // bitCount with ivec2
    return bitCount(ivec2(0, 170));
}

// run: test_bitcount_ivec2() == ivec2(0, 4)




