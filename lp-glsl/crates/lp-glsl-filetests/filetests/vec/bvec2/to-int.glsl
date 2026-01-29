// test run
// target riscv32.q32

// ============================================================================
// To Int: int(bvec2) - converts first component (false -> 0, true -> 1)
// ============================================================================

int test_bvec2_to_int_true() {
    // Conversion int(bvec2) converts first component (false -> 0, true -> 1)
    bvec2 source = bvec2(true, false);
    return int(source);
}

// run: test_bvec2_to_int_true() == 1

int test_bvec2_to_int_false() {
    bvec2 source = bvec2(false, true);
    return int(source);
}

// run: test_bvec2_to_int_false() == 0

int test_bvec2_to_int_all_true() {
    bvec2 source = bvec2(true, true);
    return int(source);
}

// run: test_bvec2_to_int_all_true() == 1

int test_bvec2_to_int_all_false() {
    bvec2 source = bvec2(false, false);
    return int(source);
}

// run: test_bvec2_to_int_all_false() == 0

int test_bvec2_to_int_variable() {
    bvec2 x = bvec2(true, false);
    return int(x);
}

// run: test_bvec2_to_int_variable() == 1

int test_bvec2_to_int_expression() {
    return int(not(bvec2(false, true)));
}

// run: test_bvec2_to_int_expression() == 1

int test_bvec2_to_int_in_arithmetic() {
    bvec2 x = bvec2(true, false);
    return int(x) + 5;
}

// run: test_bvec2_to_int_in_arithmetic() == 6
