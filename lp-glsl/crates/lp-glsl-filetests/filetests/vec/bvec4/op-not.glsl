// test run
// target riscv32.q32

// ============================================================================
// Not: not(bvec4) -> bvec4 (component-wise logical NOT)
// ============================================================================

bvec4 test_bvec4_not_all_true() {
    bvec4 a = bvec4(true, true, true, true);
    // Function not() returns bvec4 (component-wise logical NOT)
    return not(a);
}

// run: test_bvec4_not_all_true() == bvec4(false, false, false, false)

bvec4 test_bvec4_not_all_false() {
    bvec4 a = bvec4(false, false, false, false);
    return not(a);
}

// run: test_bvec4_not_all_false() == bvec4(true, true, true, true)

bvec4 test_bvec4_not_mixed() {
    bvec4 a = bvec4(true, false, true, false);
    return not(a);
}

// run: test_bvec4_not_mixed() == bvec4(false, true, false, true)

bvec4 test_bvec4_not_other_mixed() {
    bvec4 a = bvec4(false, true, false, true);
    return not(a);
}

// run: test_bvec4_not_other_mixed() == bvec4(true, false, true, false)

bvec4 test_bvec4_not_double_negation() {
    bvec4 a = bvec4(true, false, true, false);
    // Double negation should equal original
    return not(not(a));
}

// run: test_bvec4_not_double_negation() == bvec4(true, false, true, false)

bvec4 test_bvec4_not_in_expression() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    // Use equal() function for component-wise comparison (== operator does aggregate comparison)
    return equal(not(a), b);
}

// run: test_bvec4_not_in_expression() == bvec4(true, true, true, true)

bvec4 test_bvec4_not_after_assignment() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = not(a);
    b = not(b);
    return b;
}

// run: test_bvec4_not_after_assignment() == bvec4(true, false, true, false)
