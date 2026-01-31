// test run
// target riscv32.q32

// ============================================================================
// From Shortening: bvec3(bvec4) - shortening constructor
// ============================================================================

bvec3 test_bvec3_from_bvec4() {
    // Constructor bvec3(bvec4) extracts first three components
    bvec4 source = bvec4(true, false, true, false);
    return bvec3(source);
}

// run: test_bvec3_from_bvec4() == bvec3(true, false, true)

bvec3 test_bvec3_from_bvec4_all_true() {
    bvec4 source = bvec4(true, true, true, true);
    return bvec3(source);
}

// run: test_bvec3_from_bvec4_all_true() == bvec3(true, true, true)

bvec3 test_bvec3_from_bvec4_all_false() {
    bvec4 source = bvec4(false, false, false, false);
    return bvec3(source);
}

// run: test_bvec3_from_bvec4_all_false() == bvec3(false, false, false)

bvec3 test_bvec3_from_bvec4_mixed() {
    bvec4 source = bvec4(false, true, false, true);
    return bvec3(source);
}

// run: test_bvec3_from_bvec4_mixed() == bvec3(false, true, false)

bvec3 test_bvec3_from_shortening_in_expression() {
    bvec4 source = bvec4(true, false, true, false);
    return not(bvec3(source));
}

// run: test_bvec3_from_shortening_in_expression() == bvec3(false, true, false)
