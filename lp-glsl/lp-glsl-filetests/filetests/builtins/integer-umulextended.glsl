// test run
// target riscv32.q32

// ============================================================================
// umulExtended(): Unsigned multiply extended function
// umulExtended(x, y, out msb, out lsb) - unsigned multiply extended
// Produces 64-bit result: lsb = 32 least significant bits, msb = 32 most significant bits
// ============================================================================

uvec4 test_umulextended_uint_small() {
    // umulExtended(2, 3) should return (0, 6, 0, 0) -> lsb=6, msb=0
    uint msb, lsb;
    umulExtended(2u, 3u, msb, lsb);
    return uvec4(lsb, msb, 0u, 0u);
}

// run: test_umulextended_uint_small() == uvec4(6u, 0u, 0u, 0u) [expect-fail]

uvec4 test_umulextended_uint_medium() {
    // umulExtended(100000, 100000) should return (10000000000 % 2^32, 10000000000 / 2^32, 0, 0)
    uint msb, lsb;
    umulExtended(100000u, 100000u, msb, lsb);
    return uvec4(lsb, msb, 0u, 0u);
}

// run: test_umulextended_uint_medium() == uvec4(1410065408u, 2u, 0u, 0u) [expect-fail]

uvec4 test_umulextended_uint_large() {
    // umulExtended(2^16, 2^16) should return (0, 2^32, 0, 0) -> lsb=0, msb=1
    uint msb, lsb;
    umulExtended(65536u, 65536u, msb, lsb);
    return uvec4(lsb, msb, 0u, 0u);
}

// run: test_umulextended_uint_large() == uvec4(0u, 1u, 0u, 0u) [expect-fail]

uvec4 test_umulextended_uint_max() {
    // umulExtended(max_uint, max_uint) should return (1, max_uint-1, 0, 0)
    uint msb, lsb;
    umulExtended(4294967295u, 4294967295u, msb, lsb);
    return uvec4(lsb, msb, 0u, 0u);
}

// run: test_umulextended_uint_max() == uvec4(1u, 4294967294u, 0u, 0u) [expect-fail]




