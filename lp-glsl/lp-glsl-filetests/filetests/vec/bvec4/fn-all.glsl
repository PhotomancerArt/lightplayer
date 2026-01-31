// test run
// target riscv32.q32

// ============================================================================
// All: all(bvec4) -> bool (true only if all components are true)
// ============================================================================

bool test_bvec4_all_all_true() {
    bvec4 a = bvec4(true, true, true, true);
    // Function all() returns bool (true only if all components are true)
    return all(a);
}

// run: test_bvec4_all_all_true() == true

bool test_bvec4_all_all_false() {
    bvec4 a = bvec4(false, false, false, false);
    return all(a);
}

// run: test_bvec4_all_all_false() == false

bool test_bvec4_all_first_false() {
    bvec4 a = bvec4(false, true, true, true);
    return all(a);
}

// run: test_bvec4_all_first_false() == false

bool test_bvec4_all_second_false() {
    bvec4 a = bvec4(true, false, true, true);
    return all(a);
}

// run: test_bvec4_all_second_false() == false

bool test_bvec4_all_third_false() {
    bvec4 a = bvec4(true, true, false, true);
    return all(a);
}

// run: test_bvec4_all_third_false() == false

bool test_bvec4_all_fourth_false() {
    bvec4 a = bvec4(true, true, true, false);
    return all(a);
}

// run: test_bvec4_all_fourth_false() == false

bool test_bvec4_all_in_expression() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    // all(a) && all(b) should be true && true = true
    return all(a) && all(b);
}

// run: test_bvec4_all_in_expression() == true

bool test_bvec4_all_false_case() {
    bvec4 a = bvec4(true, false, true, true);
    bvec4 b = bvec4(false, true, true, true);
    // all(a) || all(b) should be false || false = false
    return all(a) || all(b);
}

// run: test_bvec4_all_false_case() == false

bool test_bvec4_all_after_operation() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 b = bvec4(true, true, true, true);
    return all(equal(a, b));
}

// run: test_bvec4_all_after_operation() == true
