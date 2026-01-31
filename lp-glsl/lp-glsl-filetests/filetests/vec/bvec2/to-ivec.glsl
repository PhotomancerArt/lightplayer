// test run
// target riscv32.q32

// ============================================================================
// To ivec: ivec2(bvec2) - component-wise conversion (false -> 0, true -> 1)
// ============================================================================

ivec2 test_bvec2_to_ivec2_mixed() {
    // Conversion ivec2(bvec2) component-wise conversion
    bvec2 source = bvec2(true, false);
    return ivec2(source);
}

// run: test_bvec2_to_ivec2_mixed() == ivec2(1, 0)

ivec2 test_bvec2_to_ivec2_all_true() {
    bvec2 source = bvec2(true, true);
    return ivec2(source);
}

// run: test_bvec2_to_ivec2_all_true() == ivec2(1, 1)

ivec2 test_bvec2_to_ivec2_all_false() {
    bvec2 source = bvec2(false, false);
    return ivec2(source);
}

// run: test_bvec2_to_ivec2_all_false() == ivec2(0, 0)

ivec2 test_bvec2_to_ivec2_other_mixed() {
    bvec2 source = bvec2(false, true);
    return ivec2(source);
}

// run: test_bvec2_to_ivec2_other_mixed() == ivec2(0, 1)

ivec2 test_bvec2_to_ivec2_variable() {
    bvec2 x = bvec2(true, false);
    return ivec2(x);
}

// run: test_bvec2_to_ivec2_variable() == ivec2(1, 0)

ivec2 test_bvec2_to_ivec2_expression() {
    return ivec2(not(bvec2(false, true)));
}

// run: test_bvec2_to_ivec2_expression() == ivec2(1, 0)

ivec2 test_bvec2_to_ivec2_in_arithmetic() {
    bvec2 x = bvec2(true, false);
    return ivec2(x) + ivec2(10, 20);
}

// run: test_bvec2_to_ivec2_in_arithmetic() == ivec2(11, 20)
