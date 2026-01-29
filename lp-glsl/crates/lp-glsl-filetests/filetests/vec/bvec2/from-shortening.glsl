// test run
// target riscv32.q32

// ============================================================================
// From Shortening: bvec2(bvec3), bvec2(bvec4) - shortening constructors
// ============================================================================

bvec2 test_bvec2_from_bvec3() {
    // Constructor bvec2(bvec3) extracts first two components
    bvec3 source = bvec3(true, false, true);
    return bvec2(source);
}

// run: test_bvec2_from_bvec3() == bvec2(true, false)

bvec2 test_bvec2_from_bvec4() {
    // Constructor bvec2(bvec4) extracts first two components
    bvec4 source = bvec4(true, false, true, false);
    return bvec2(source);
}

// run: test_bvec2_from_bvec4() == bvec2(true, false)

bvec2 test_bvec2_from_bvec3_all_true() {
    bvec3 source = bvec3(true, true, true);
    return bvec2(source);
}

// run: test_bvec2_from_bvec3_all_true() == bvec2(true, true)

bvec2 test_bvec2_from_bvec3_all_false() {
    bvec3 source = bvec3(false, false, false);
    return bvec2(source);
}

// run: test_bvec2_from_bvec3_all_false() == bvec2(false, false)

bvec2 test_bvec2_from_bvec4_mixed() {
    bvec4 source = bvec4(false, true, false, true);
    return bvec2(source);
}

// run: test_bvec2_from_bvec4_mixed() == bvec2(false, true)

bvec2 test_bvec2_from_shortening_in_expression() {
    bvec3 source = bvec3(true, false, true);
    return not(bvec2(source));
}

// run: test_bvec2_from_shortening_in_expression() == bvec2(false, true)
