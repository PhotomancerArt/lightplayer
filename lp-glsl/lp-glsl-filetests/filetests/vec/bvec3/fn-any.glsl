// test run
// target riscv32.q32

// ============================================================================
// Any: any(bvec3) -> bool (true if any component is true)
// ============================================================================

bool test_bvec3_any_all_true() {
    bvec3 a = bvec3(true, true, true);
    // Function any() returns bool (true if any component is true)
    return any(a);
}

// run: test_bvec3_any_all_true() == true

bool test_bvec3_any_all_false() {
    bvec3 a = bvec3(false, false, false);
    return any(a);
}

// run: test_bvec3_any_all_false() == false

bool test_bvec3_any_first_true() {
    bvec3 a = bvec3(true, false, false);
    return any(a);
}

// run: test_bvec3_any_first_true() == true

bool test_bvec3_any_second_true() {
    bvec3 a = bvec3(false, true, false);
    return any(a);
}

// run: test_bvec3_any_second_true() == true

bool test_bvec3_any_third_true() {
    bvec3 a = bvec3(false, false, true);
    return any(a);
}

// run: test_bvec3_any_third_true() == true

bool test_bvec3_any_mixed_true() {
    bvec3 a = bvec3(false, true, false);
    return any(a);
}

// run: test_bvec3_any_mixed_true() == true

bool test_bvec3_any_in_expression() {
    bvec3 a = bvec3(true, false, false);
    bvec3 b = bvec3(false, true, false);
    // any(a) && any(b) should be true && true = true
    return any(a) && any(b);
}

// run: test_bvec3_any_in_expression() == true

bool test_bvec3_any_false_case() {
    bvec3 a = bvec3(false, false, false);
    bvec3 b = bvec3(false, false, false);
    // any(a) || any(b) should be false || false = false
    return any(a) || any(b);
}

// run: test_bvec3_any_false_case() == false

bool test_bvec3_any_after_operation() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, true);
    return any(equal(a, b));
}

// run: test_bvec3_any_after_operation() == true
