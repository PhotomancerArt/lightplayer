// test run
// target riscv32.q32

// ============================================================================
// From bvec: bvec4(bvec4) - identity constructor
// ============================================================================

bvec4 test_bvec4_from_bvec4_identity() {
    // Constructor bvec4(bvec4) is identity constructor
    bvec4 source = bvec4(true, false, true, false);
    return bvec4(source);
}

// run: test_bvec4_from_bvec4_identity() == bvec4(true, false, true, false)

bvec4 test_bvec4_from_bvec4_all_true() {
    bvec4 source = bvec4(true, true, true, true);
    return bvec4(source);
}

// run: test_bvec4_from_bvec4_all_true() == bvec4(true, true, true, true)

bvec4 test_bvec4_from_bvec4_all_false() {
    bvec4 source = bvec4(false, false, false, false);
    return bvec4(source);
}

// run: test_bvec4_from_bvec4_all_false() == bvec4(false, false, false, false)

bvec4 test_bvec4_from_bvec4_variable() {
    bvec4 x = bvec4(true, false, true, false);
    return bvec4(x);
}

// run: test_bvec4_from_bvec4_variable() == bvec4(true, false, true, false)

bvec4 test_bvec4_from_bvec4_expression() {
    return bvec4(not(bvec4(false, true, false, true)));
}

// run: test_bvec4_from_bvec4_expression() == bvec4(true, false, true, false)

bvec4 test_bvec4_from_bvec4_in_assignment() {
    bvec4 source = bvec4(true, false, true, false);
    bvec4 result;
    result = bvec4(source);
    return result;
}

// run: test_bvec4_from_bvec4_in_assignment() == bvec4(true, false, true, false)

bvec4 main() {
    return test_bvec4_from_bvec4_in_assignment();
}

