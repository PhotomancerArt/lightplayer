// test run
// target riscv32.q32

// ============================================================================
// Any: any(bvec2) -> bool (true if any component is true)
// ============================================================================

bool test_bvec2_any_all_true() {
    bvec2 a = bvec2(true, true);
    // Function any() returns bool (true if any component is true)
    return any(a);
}

// run: test_bvec2_any_all_true() == true

bool test_bvec2_any_all_false() {
    bvec2 a = bvec2(false, false);
    return any(a);
}

// run: test_bvec2_any_all_false() == false

bool test_bvec2_any_first_true() {
    bvec2 a = bvec2(true, false);
    return any(a);
}

// run: test_bvec2_any_first_true() == true

bool test_bvec2_any_second_true() {
    bvec2 a = bvec2(false, true);
    return any(a);
}

// run: test_bvec2_any_second_true() == true

bool test_bvec2_any_in_expression() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    // any(a) && any(b) should be true && true = true
    return any(a) && any(b);
}

// run: test_bvec2_any_in_expression() == true

bool test_bvec2_any_false_case() {
    bvec2 a = bvec2(false, false);
    bvec2 b = bvec2(false, false);
    // any(a) || any(b) should be false || false = false
    return any(a) || any(b);
}

// run: test_bvec2_any_false_case() == false

bool test_bvec2_any_after_operation() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    return any(equal(a, not(b)));
}

// run: test_bvec2_any_after_operation() == true
