// test run
// target riscv32.q32

// ============================================================================
// From bvec: bvec2(bvec2) - identity constructor
// ============================================================================

bvec2 test_bvec2_from_bvec2_identity() {
    // Constructor bvec2(bvec2) is identity constructor
    bvec2 source = bvec2(true, false);
    return bvec2(source);
}

// run: test_bvec2_from_bvec2_identity() == bvec2(true, false)

bvec2 test_bvec2_from_bvec2_all_true() {
    bvec2 source = bvec2(true, true);
    return bvec2(source);
}

// run: test_bvec2_from_bvec2_all_true() == bvec2(true, true)

bvec2 test_bvec2_from_bvec2_all_false() {
    bvec2 source = bvec2(false, false);
    return bvec2(source);
}

// run: test_bvec2_from_bvec2_all_false() == bvec2(false, false)

bvec2 test_bvec2_from_bvec2_variable() {
    bvec2 x = bvec2(true, false);
    return bvec2(x);
}

// run: test_bvec2_from_bvec2_variable() == bvec2(true, false)

bvec2 test_bvec2_from_bvec2_expression() {
    return bvec2(not(bvec2(false, true)));
}

// run: test_bvec2_from_bvec2_expression() == bvec2(true, false)

bvec2 test_bvec2_from_bvec2_in_assignment() {
    bvec2 source = bvec2(true, false);
    bvec2 result;
    result = bvec2(source);
    return result;
}

// run: test_bvec2_from_bvec2_in_assignment() == bvec2(true, false)
