// test run
// target riscv32.q32

// ============================================================================
// All: all(bvec3) -> bool (true only if all components are true)
// ============================================================================

bool test_bvec3_all_all_true() {
    bvec3 a = bvec3(true, true, true);
    // Function all() returns bool (true only if all components are true)
    return all(a);
}

// run: test_bvec3_all_all_true() == true

bool test_bvec3_all_all_false() {
    bvec3 a = bvec3(false, false, false);
    return all(a);
}

// run: test_bvec3_all_all_false() == false

bool test_bvec3_all_first_false() {
    bvec3 a = bvec3(false, true, true);
    return all(a);
}

// run: test_bvec3_all_first_false() == false

bool test_bvec3_all_second_false() {
    bvec3 a = bvec3(true, false, true);
    return all(a);
}

// run: test_bvec3_all_second_false() == false

bool test_bvec3_all_third_false() {
    bvec3 a = bvec3(true, true, false);
    return all(a);
}

// run: test_bvec3_all_third_false() == false

bool test_bvec3_all_in_expression() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    // all(a) && all(b) should be true && true = true
    return all(a) && all(b);
}

// run: test_bvec3_all_in_expression() == true

bool test_bvec3_all_false_case() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, true);
    // all(a) || all(b) should be false || false = false
    return all(a) || all(b);
}

// run: test_bvec3_all_false_case() == false

bool test_bvec3_all_after_operation() {
    bvec3 a = bvec3(true, true, true);
    bvec3 b = bvec3(true, true, true);
    return all(equal(a, b));
}

// run: test_bvec3_all_after_operation() == true
