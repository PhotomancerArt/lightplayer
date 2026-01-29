// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(bvec4, bvec4) -> bvec4 (component-wise)
// ============================================================================

bool test_bvec4_equal_operator_true() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_bvec4_equal_operator_true() == true

bool test_bvec4_equal_operator_false() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    return a == b;
}

// run: test_bvec4_equal_operator_false() == false

bool test_bvec4_equal_operator_partial_match() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(true, false, true, true);
    return a == b;
}

// run: test_bvec4_equal_operator_partial_match() == false

bool test_bvec4_equal_operator_all_false() {
    bvec4 a = bvec4(false, false, false, false);
    bvec4 b = bvec4(false, false, false, false);
    return a == b;
}

// run: test_bvec4_equal_operator_all_false() == true

bvec4 test_bvec4_equal_function() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(true, true, true, false);
    // Function equal() returns bvec4 (component-wise comparison)
    return equal(a, b);
}

// run: test_bvec4_equal_function() == bvec4(true, false, true, true)

bvec4 test_bvec4_equal_function_all_true() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    return equal(a, b);
}

// run: test_bvec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_bvec4_equal_function_all_false() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    return equal(a, b);
}

// run: test_bvec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_bvec4_equal_function_mixed() {
    bvec4 a = bvec4(false, true, false, true);
    bvec4 b = bvec4(true, true, false, true);
    return equal(a, b);
}

// run: test_bvec4_equal_function_mixed() == bvec4(false, true, true, true)

bool test_bvec4_equal_operator_after_assignment() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    b = a;
    return a == b;
}

// run: test_bvec4_equal_operator_after_assignment() == true
