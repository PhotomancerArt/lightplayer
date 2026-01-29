// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(bvec3, bvec3) -> bvec3 (component-wise)
// ============================================================================

bool test_bvec3_equal_operator_true() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_bvec3_equal_operator_true() == true

bool test_bvec3_equal_operator_false() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    return a == b;
}

// run: test_bvec3_equal_operator_false() == false

bool test_bvec3_equal_operator_partial_match() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(true, false, false);
    return a == b;
}

// run: test_bvec3_equal_operator_partial_match() == false

bool test_bvec3_equal_operator_all_false() {
    bvec3 a = bvec3(false, false, false);
    bvec3 b = bvec3(false, false, false);
    return a == b;
}

// run: test_bvec3_equal_operator_all_false() == true

bvec3 test_bvec3_equal_function() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(true, true, true);
    // Function equal() returns bvec3 (component-wise comparison)
    return equal(a, b);
}

// run: test_bvec3_equal_function() == bvec3(true, false, true)

bvec3 test_bvec3_equal_function_all_true() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    return equal(a, b);
}

// run: test_bvec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_bvec3_equal_function_all_false() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    return equal(a, b);
}

// run: test_bvec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_bvec3_equal_function_mixed() {
    bvec3 a = bvec3(false, true, false);
    bvec3 b = bvec3(true, true, false);
    return equal(a, b);
}

// run: test_bvec3_equal_function_mixed() == bvec3(false, true, true)

bool test_bvec3_equal_operator_after_assignment() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    b = a;
    return a == b;
}

// run: test_bvec3_equal_operator_after_assignment() == true
