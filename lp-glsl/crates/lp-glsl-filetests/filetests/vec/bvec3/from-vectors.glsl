// test run
// target riscv32.q32

// ============================================================================
// From Vectors: bvec3(bvec2, bool), bvec3(bool, bvec2) - vector combinations
// ============================================================================

bvec3 test_bvec3_from_bvec2_bool() {
    // Constructor bvec3(bvec2, bool) combines bvec2 and bool
    bvec2 source = bvec2(true, false);
    bool third = true;
    return bvec3(source, third);
}

// run: test_bvec3_from_bvec2_bool() == bvec3(true, false, true)

bvec3 test_bvec3_from_bool_bvec2() {
    // Constructor bvec3(bool, bvec2) combines bool and bvec2
    bool first = false;
    bvec2 source = bvec2(true, true);
    return bvec3(first, source);
}

// run: test_bvec3_from_bool_bvec2() == bvec3(false, true, true)

bvec3 test_bvec3_from_bvec2_bool_false() {
    bvec2 source = bvec2(false, true);
    bool third = false;
    return bvec3(source, third);
}

// run: test_bvec3_from_bvec2_bool_false() == bvec3(false, true, false)

bvec3 test_bvec3_from_bool_bvec2_false() {
    bool first = true;
    bvec2 source = bvec2(false, false);
    return bvec3(first, source);
}

// run: test_bvec3_from_bool_bvec2_false() == bvec3(true, false, false)

bvec3 test_bvec3_from_vectors_expressions() {
    return bvec3(not(bvec2(false, true)), true);
}

// run: test_bvec3_from_vectors_expressions() == bvec3(true, false, true)

bvec3 test_bvec3_from_vectors_other_combination() {
    return bvec3(false, not(bvec2(true, false)));
}

// run: test_bvec3_from_vectors_other_combination() == bvec3(false, false, true)

bvec3 test_bvec3_from_vectors_in_assignment() {
    bvec3 result;
    bvec2 source = bvec2(true, true);
    result = bvec3(source, false);
    return result;
}

// run: test_bvec3_from_vectors_in_assignment() == bvec3(true, true, false)
