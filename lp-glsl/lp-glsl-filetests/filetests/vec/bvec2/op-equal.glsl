// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(bvec2, bvec2) -> bvec2 (component-wise)
// ============================================================================

bool test_bvec2_equal_operator_true() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_bvec2_equal_operator_true() == true

bool test_bvec2_equal_operator_false() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    return a == b;
}

// run: test_bvec2_equal_operator_false() == false

bool test_bvec2_equal_operator_partial_match() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(true, true);
    return a == b;
}

// run: test_bvec2_equal_operator_partial_match() == false

bool test_bvec2_equal_operator_all_false() {
    bvec2 a = bvec2(false, false);
    bvec2 b = bvec2(false, false);
    return a == b;
}

// run: test_bvec2_equal_operator_all_false() == true

bvec2 test_bvec2_equal_function() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(true, true);
    // Function equal() returns bvec2 (component-wise comparison)
    return equal(a, b);
}

// run: test_bvec2_equal_function() == bvec2(true, false)

bvec2 test_bvec2_equal_function_all_true() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    return equal(a, b);
}

// run: test_bvec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_bvec2_equal_function_all_false() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    return equal(a, b);
}

// run: test_bvec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_bvec2_equal_function_mixed() {
    bvec2 a = bvec2(false, true);
    bvec2 b = bvec2(true, true);
    return equal(a, b);
}

// run: test_bvec2_equal_function_mixed() == bvec2(false, true)

bool test_bvec2_equal_operator_after_assignment() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    b = a;
    return a == b;
}

// run: test_bvec2_equal_operator_after_assignment() == true
