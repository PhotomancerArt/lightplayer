// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(bvec3, bvec3) -> bvec3 (component-wise)
// ============================================================================

bool test_bvec3_not_equal_operator_true() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_bvec3_not_equal_operator_true() == true

bool test_bvec3_not_equal_operator_false() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    return a != b;
}

// run: test_bvec3_not_equal_operator_false() == false

bool test_bvec3_not_equal_operator_partial_match() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(true, false, false);
    return a != b;
}

// run: test_bvec3_not_equal_operator_partial_match() == true

bool test_bvec3_not_equal_operator_all_false() {
    bvec3 a = bvec3(false, false, false);
    bvec3 b = bvec3(true, true, true);
    return a != b;
}

// run: test_bvec3_not_equal_operator_all_false() == true

bvec3 test_bvec3_not_equal_function() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(true, true, true);
    // Function notEqual() returns bvec3 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_bvec3_not_equal_function() == bvec3(false, true, false)

bvec3 test_bvec3_not_equal_function_all_false() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    return notEqual(a, b);
}

// run: test_bvec3_not_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_bvec3_not_equal_function_all_true() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    return notEqual(a, b);
}

// run: test_bvec3_not_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_bvec3_not_equal_function_mixed() {
    bvec3 a = bvec3(false, true, false);
    bvec3 b = bvec3(true, true, false);
    return notEqual(a, b);
}

// run: test_bvec3_not_equal_function_mixed() == bvec3(true, false, false)

bool test_bvec3_not_equal_operator_self() {
    bvec3 a = bvec3(true, false, true);
    return a != a;
}

// run: test_bvec3_not_equal_operator_self() == false
