// test run
// target riscv32.q32

// ============================================================================
// findLSB(): Find least significant bit function
// findLSB(value) - returns bit number of least significant one bit
// Returns -1 if value is 0
// ============================================================================

int test_findlsb_int_zero() {
    // findLSB(0) should be -1
    return findLSB(0);
}

// run: test_findlsb_int_zero() == -1 [expect-fail]

int test_findlsb_int_one() {
    // findLSB(1) should be 0 (bit 0 is set)
    return findLSB(1);
}

// run: test_findlsb_int_one() == 0 [expect-fail]

int test_findlsb_int_two() {
    // findLSB(2) should be 1 (bit 1 is set: 0b10)
    return findLSB(2);
}

// run: test_findlsb_int_two() == 1 [expect-fail]

int test_findlsb_int_four() {
    // findLSB(4) should be 2 (bit 2 is set: 0b100)
    return findLSB(4);
}

// run: test_findlsb_int_four() == 2 [expect-fail]

int test_findlsb_int_pattern() {
    // findLSB(0b10101000) should be 3 (bit 3 is the least significant one)
    return findLSB(168);
}

// run: test_findlsb_int_pattern() == 3 [expect-fail]

uint test_findlsb_uint_zero() {
    // findLSB(uint(0)) should be -1, but returns uint so check against 4294967295u (-1 as uint)
    return findLSB(0u);
}

// run: test_findlsb_uint_zero() == 4294967295u [expect-fail]

uint test_findlsb_uint_one() {
    // findLSB(uint(1)) should be 0
    return findLSB(1u);
}

// run: test_findlsb_uint_one() == 0u [expect-fail]

uint test_findlsb_uint_pattern() {
    // findLSB(uint(168)) should be 3
    return findLSB(168u);
}

// run: test_findlsb_uint_pattern() == 3u [expect-fail]

ivec2 test_findlsb_ivec2() {
    // findLSB with ivec2
    return findLSB(ivec2(0, 168));
}

// run: test_findlsb_ivec2() == ivec2(-1, 3) [expect-fail]




