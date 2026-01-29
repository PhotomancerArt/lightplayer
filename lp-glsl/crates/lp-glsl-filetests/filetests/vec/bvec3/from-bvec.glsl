// test run
// target riscv32.q32

// ============================================================================
// From bvec: bvec3(bvec3) - identity constructor
// ============================================================================

bvec3 test_bvec3_from_bvec3_identity() {
    // Constructor bvec3(bvec3) is identity constructor
    bvec3 source = bvec3(true, false, true);
    return bvec3(source);
}

// run: test_bvec3_from_bvec3_identity() == bvec3(true, false, true)

bvec3 test_bvec3_from_bvec3_all_true() {
    bvec3 source = bvec3(true, true, true);
    return bvec3(source);
}

// run: test_bvec3_from_bvec3_all_true() == bvec3(true, true, true)

bvec3 test_bvec3_from_bvec3_all_false() {
    bvec3 source = bvec3(false, false, false);
    return bvec3(source);
}

// run: test_bvec3_from_bvec3_all_false() == bvec3(false, false, false)

bvec3 test_bvec3_from_bvec3_variable() {
    bvec3 x = bvec3(true, false, true);
    return bvec3(x);
}

// run: test_bvec3_from_bvec3_variable() == bvec3(true, false, true)

bvec3 test_bvec3_from_bvec3_expression() {
    return bvec3(not(bvec3(false, true, false)));
}

// run: test_bvec3_from_bvec3_expression() == bvec3(true, false, true)

bvec3 test_bvec3_from_bvec3_in_assignment() {
    bvec3 source = bvec3(true, false, true);
    bvec3 result;
    result = bvec3(source);
    return result;
}

// run: test_bvec3_from_bvec3_in_assignment() == bvec3(true, false, true)
