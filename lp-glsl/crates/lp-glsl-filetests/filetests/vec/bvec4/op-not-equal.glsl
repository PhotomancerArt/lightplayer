// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(bvec4, bvec4) -> bvec4 (component-wise)
// ============================================================================

bool test_bvec4_not_equal_operator_true() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_bvec4_not_equal_operator_true() == true

bool test_bvec4_not_equal_operator_false() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    return a != b;
}

// run: test_bvec4_not_equal_operator_false() == false

bool test_bvec4_not_equal_operator_partial_match() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(true, false, true, true);
    return a != b;
}

// run: test_bvec4_not_equal_operator_partial_match() == true

bool test_bvec4_not_equal_operator_all_false() {
    bvec4 a = bvec4(false, false, false, false);
    bvec4 b = bvec4(true, true, true, true);
    return a != b;
}

// run: test_bvec4_not_equal_operator_all_false() == true

bvec4 test_bvec4_not_equal_function() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(true, true, true, false);
    // Function notEqual() returns bvec4 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_bvec4_not_equal_function() == bvec4(false, true, false, false)

bvec4 test_bvec4_not_equal_function_all_false() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    return notEqual(a, b);
}

// run: test_bvec4_not_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_bvec4_not_equal_function_all_true() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    return notEqual(a, b);
}

// run: test_bvec4_not_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_bvec4_not_equal_function_mixed() {
    bvec4 a = bvec4(false, true, false, true);
    bvec4 b = bvec4(true, true, false, true);
    return notEqual(a, b);
}

// run: test_bvec4_not_equal_function_mixed() == bvec4(true, false, false, false)

bool test_bvec4_not_equal_operator_self() {
    bvec4 a = bvec4(true, false, true, false);
    return a != a;
}

// run: test_bvec4_not_equal_operator_self() == false
