// test run
// target riscv32.q32

// ============================================================================
// bitCount(): Bit count function
// bitCount(value) - returns number of one bits
// ============================================================================

int test_bitcount_int_zero() {
    // bitCount(0) should be 0
    return bitCount(0);
}

// run: test_bitcount_int_zero() == 0 [expect-fail]

int test_bitcount_int_one() {
    // bitCount(1) should be 1
    return bitCount(1);
}

// run: test_bitcount_int_one() == 1 [expect-fail]

int test_bitcount_int_full() {
    // bitCount(-1) should be 32 (all bits set)
    return bitCount(-1);
}

// run: test_bitcount_int_full() == 32 [expect-fail]

int test_bitcount_int_pattern() {
    // bitCount(0b10101010) should be 4 (4 ones)
    return bitCount(170);
}

// run: test_bitcount_int_pattern() == 4 [expect-fail]

uint test_bitcount_uint_zero() {
    // bitCount(uint(0)) should be 0
    return bitCount(0u);
}

// run: test_bitcount_uint_zero() == 0 [expect-fail]

uint test_bitcount_uint_one() {
    // bitCount(uint(1)) should be 1
    return bitCount(1u);
}

// run: test_bitcount_uint_one() == 1 [expect-fail]

uint test_bitcount_uint_full() {
    // bitCount(uint max) should be 32
    return bitCount(4294967295u);
}

// run: test_bitcount_uint_full() == 32 [expect-fail]

uint test_bitcount_uint_pattern() {
    // bitCount(uint(170)) should be 4
    return bitCount(170u);
}

// run: test_bitcount_uint_pattern() == 4 [expect-fail]

ivec2 test_bitcount_ivec2() {
    // bitCount with ivec2
    return bitCount(ivec2(0, 170));
}

// run: test_bitcount_ivec2() == ivec2(0, 4) [expect-fail]




