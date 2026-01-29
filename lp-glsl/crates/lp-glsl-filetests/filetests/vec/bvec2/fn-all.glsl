// test run
// target riscv32.q32

// ============================================================================
// All: all(bvec2) -> bool (true only if all components are true)
// ============================================================================

bool test_bvec2_all_all_true() {
    bvec2 a = bvec2(true, true);
    // Function all() returns bool (true only if all components are true)
    return all(a);
}

// run: test_bvec2_all_all_true() == true

bool test_bvec2_all_all_false() {
    bvec2 a = bvec2(false, false);
    return all(a);
}

// run: test_bvec2_all_all_false() == false

bool test_bvec2_all_first_false() {
    bvec2 a = bvec2(false, true);
    return all(a);
}

// run: test_bvec2_all_first_false() == false

bool test_bvec2_all_second_false() {
    bvec2 a = bvec2(true, false);
    return all(a);
}

// run: test_bvec2_all_second_false() == false

bool test_bvec2_all_in_expression() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    // all(a) && all(b) should be true && true = true
    return all(a) && all(b);
}

// run: test_bvec2_all_in_expression() == true

bool test_bvec2_all_false_case() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    // all(a) || all(b) should be false || false = false
    return all(a) || all(b);
}

// run: test_bvec2_all_false_case() == false

bool test_bvec2_all_after_operation() {
    bvec2 a = bvec2(true, true);
    bvec2 b = bvec2(true, true);
    return all(equal(a, b));
}

// run: test_bvec2_all_after_operation() == true
