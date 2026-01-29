// test run
// target riscv32.q32

// ============================================================================
// findMSB(): Find most significant bit function
// findMSB(value) - returns bit number of most significant bit
// For positive: most significant one bit
// For negative: most significant zero bit
// Returns -1 if value is 0 or -1
// ============================================================================

int test_findmsb_int_zero() {
    // findMSB(0) should be -1
    return findMSB(0);
}

// run: test_findmsb_int_zero() == -1 [expect-fail]

int test_findmsb_int_one() {
    // findMSB(1) should be 0 (bit 0 is the most significant one)
    return findMSB(1);
}

// run: test_findmsb_int_one() == 0 [expect-fail]

int test_findmsb_int_two() {
    // findMSB(2) should be 1 (bit 1 is the most significant one: 0b10)
    return findMSB(2);
}

// run: test_findmsb_int_two() == 1 [expect-fail]

int test_findmsb_int_neg_one() {
    // findMSB(-1) should be -1 (all bits set)
    return findMSB(-1);
}

// run: test_findmsb_int_neg_one() == -1 [expect-fail]

int test_findmsb_int_negative() {
    // findMSB(-2) should be 30 (most significant zero bit in two's complement)
    return findMSB(-2);
}

// run: test_findmsb_int_negative() == 30 [expect-fail]

int test_findmsb_int_large() {
    // findMSB(2147483648) should be 31 (bit 31 is set: 2^31)
    return findMSB(2147483648);
}

// run: test_findmsb_int_large() == 31 [expect-fail]

uint test_findmsb_uint_zero() {
    // findMSB(uint(0)) should be -1, but returns uint so check against 4294967295u (-1 as uint)
    return findMSB(0u);
}

// run: test_findmsb_uint_zero() == 4294967295u [expect-fail]

uint test_findmsb_uint_one() {
    // findMSB(uint(1)) should be 0
    return findMSB(1u);
}

// run: test_findmsb_uint_one() == 0u [expect-fail]

uint test_findmsb_uint_large() {
    // findMSB(uint(2147483648)) should be 31
    return findMSB(2147483648u);
}

// run: test_findmsb_uint_large() == 31u [expect-fail]

ivec2 test_findmsb_ivec2() {
    // findMSB with ivec2
    return findMSB(ivec2(0, 2));
}

// run: test_findmsb_ivec2() == ivec2(-1, 1) [expect-fail]




