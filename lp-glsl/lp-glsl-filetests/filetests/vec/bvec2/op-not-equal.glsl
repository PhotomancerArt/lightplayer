// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(bvec2, bvec2) -> bvec2 (component-wise)
// ============================================================================

bool test_bvec2_not_equal_operator_true() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_bvec2_not_equal_operator_true() == true

bool test_bvec2_not_equal_operator_false() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    return a != b;
}

// run: test_bvec2_not_equal_operator_false() == false

bool test_bvec2_not_equal_operator_partial_match() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(true, true);
    return a != b;
}

// run: test_bvec2_not_equal_operator_partial_match() == true

bool test_bvec2_not_equal_operator_all_false() {
    bvec2 a = bvec2(false, false);
    bvec2 b = bvec2(true, true);
    return a != b;
}

// run: test_bvec2_not_equal_operator_all_false() == true

bvec2 test_bvec2_not_equal_function() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(true, true);
    // Function notEqual() returns bvec2 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_bvec2_not_equal_function() == bvec2(false, true)

bvec2 test_bvec2_not_equal_function_all_false() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    return notEqual(a, b);
}

// run: test_bvec2_not_equal_function_all_false() == bvec2(false, false)

bvec2 test_bvec2_not_equal_function_all_true() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    return notEqual(a, b);
}

// run: test_bvec2_not_equal_function_all_true() == bvec2(true, true)

bvec2 test_bvec2_not_equal_function_mixed() {
    bvec2 a = bvec2(false, true);
    bvec2 b = bvec2(true, true);
    return notEqual(a, b);
}

// run: test_bvec2_not_equal_function_mixed() == bvec2(true, false)

bool test_bvec2_not_equal_operator_self() {
    bvec2 a = bvec2(true, false);
    return a != a;
}

// run: test_bvec2_not_equal_operator_self() == false
