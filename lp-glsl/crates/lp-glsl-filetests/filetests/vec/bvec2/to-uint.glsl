// test run
// target riscv32.q32

// ============================================================================
// To Uint: uint(bvec2) - converts first component (false -> 0u, true -> 1u)
// ============================================================================

uint test_bvec2_to_uint_true() {
    // Conversion uint(bvec2) converts first component (false -> 0u, true -> 1u)
    bvec2 source = bvec2(true, false);
    return uint(source);
}

// run: test_bvec2_to_uint_true() == 1u

uint test_bvec2_to_uint_false() {
    bvec2 source = bvec2(false, true);
    return uint(source);
}

// run: test_bvec2_to_uint_false() == 0u

uint test_bvec2_to_uint_all_true() {
    bvec2 source = bvec2(true, true);
    return uint(source);
}

// run: test_bvec2_to_uint_all_true() == 1u

uint test_bvec2_to_uint_all_false() {
    bvec2 source = bvec2(false, false);
    return uint(source);
}

// run: test_bvec2_to_uint_all_false() == 0u

uint test_bvec2_to_uint_variable() {
    bvec2 x = bvec2(true, false);
    return uint(x);
}

// run: test_bvec2_to_uint_variable() == 1u

uint test_bvec2_to_uint_expression() {
    return uint(not(bvec2(false, true)));
}

// run: test_bvec2_to_uint_expression() == 1u

uint test_bvec2_to_uint_in_arithmetic() {
    bvec2 x = bvec2(true, false);
    return uint(x) + 5u;
}

// run: test_bvec2_to_uint_in_arithmetic() == 6u
