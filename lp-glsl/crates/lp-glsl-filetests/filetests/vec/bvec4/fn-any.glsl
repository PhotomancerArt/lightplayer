// test run
// target riscv32.q32

// ============================================================================
// Any: any(bvec4) -> bool (true if any component is true)
// ============================================================================

bool test_bvec4_any_all_true() {
    bvec4 a = bvec4(true, true, true, true);
    // Function any() returns bool (true if any component is true)
    return any(a);
}

// run: test_bvec4_any_all_true() == true

bool test_bvec4_any_all_false() {
    bvec4 a = bvec4(false, false, false, false);
    return any(a);
}

// run: test_bvec4_any_all_false() == false

bool test_bvec4_any_first_true() {
    bvec4 a = bvec4(true, false, false, false);
    return any(a);
}

// run: test_bvec4_any_first_true() == true

bool test_bvec4_any_second_true() {
    bvec4 a = bvec4(false, true, false, false);
    return any(a);
}

// run: test_bvec4_any_second_true() == true

bool test_bvec4_any_third_true() {
    bvec4 a = bvec4(false, false, true, false);
    return any(a);
}

// run: test_bvec4_any_third_true() == true

bool test_bvec4_any_fourth_true() {
    bvec4 a = bvec4(false, false, false, true);
    return any(a);
}

// run: test_bvec4_any_fourth_true() == true

bool test_bvec4_any_mixed_true() {
    bvec4 a = bvec4(false, true, false, false);
    return any(a);
}

// run: test_bvec4_any_mixed_true() == true

bool test_bvec4_any_in_expression() {
    bvec4 a = bvec4(true, false, false, false);
    bvec4 b = bvec4(false, true, false, false);
    // any(a) && any(b) should be true && true = true
    return any(a) && any(b);
}

// run: test_bvec4_any_in_expression() == true

bool test_bvec4_any_false_case() {
    bvec4 a = bvec4(false, false, false, false);
    bvec4 b = bvec4(false, false, false, false);
    // any(a) || any(b) should be false || false = false
    return any(a) || any(b);
}

// run: test_bvec4_any_false_case() == false

bool test_bvec4_any_after_operation() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, true, true);
    return any(equal(a, b));
}

// run: test_bvec4_any_after_operation() == true
