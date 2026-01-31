// test run
// target riscv32.q32

// ============================================================================
// imulExtended(): Signed multiply extended function
// imulExtended(x, y, out msb, out lsb) - signed multiply extended
// Produces 64-bit result: lsb = 32 least significant bits, msb = 32 most significant bits
// ============================================================================

uvec4 test_imulextended_int_small() {
    // imulExtended(2, 3) should return (0, 6, 0, 0) -> lsb=6, msb=0
    int msb, lsb;
    imulExtended(2, 3, msb, lsb);
    return uvec4(uint(lsb), uint(msb), 0u, 0u);
}

// run: test_imulextended_int_small() == uvec4(6u, 0u, 0u, 0u) [expect-fail]

uvec4 test_imulextended_int_neg_pos() {
    // imulExtended(-2, 3) should return (0, -6, 0, 0) -> lsb=-6, msb=-1 (sign extension)
    int msb, lsb;
    imulExtended(-2, 3, msb, lsb);
    return uvec4(uint(lsb), uint(msb), 0u, 0u);
}

// run: test_imulextended_int_neg_pos() == uvec4(4294967290u, 4294967295u, 0u, 0u) [expect-fail]

uvec4 test_imulextended_int_neg_neg() {
    // imulExtended(-2, -3) should return (0, 6, 0, 0) -> lsb=6, msb=0
    int msb, lsb;
    imulExtended(-2, -3, msb, lsb);
    return uvec4(uint(lsb), uint(msb), 0u, 0u);
}

// run: test_imulextended_int_neg_neg() == uvec4(6u, 0u, 0u, 0u) [expect-fail]

uvec4 test_imulextended_int_large() {
    // imulExtended(100000, 100000) should return same as unsigned version
    int msb, lsb;
    imulExtended(100000, 100000, msb, lsb);
    return uvec4(uint(lsb), uint(msb), 0u, 0u);
}

// run: test_imulextended_int_large() == uvec4(1410065408u, 2u, 0u, 0u) [expect-fail]




