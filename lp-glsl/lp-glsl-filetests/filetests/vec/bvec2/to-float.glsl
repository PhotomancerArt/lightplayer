// test run
// target riscv32.q32

// ============================================================================
// To Float: float(bvec2) - converts first component (false -> 0.0, true -> 1.0)
// ============================================================================

float test_bvec2_to_float_true() {
    // Conversion float(bvec2) converts first component (false -> 0.0, true -> 1.0)
    bvec2 source = bvec2(true, false);
    return float(source);
}

// run: test_bvec2_to_float_true() ~= 1.0

float test_bvec2_to_float_false() {
    bvec2 source = bvec2(false, true);
    return float(source);
}

// run: test_bvec2_to_float_false() ~= 0.0

float test_bvec2_to_float_all_true() {
    bvec2 source = bvec2(true, true);
    return float(source);
}

// run: test_bvec2_to_float_all_true() ~= 1.0

float test_bvec2_to_float_all_false() {
    bvec2 source = bvec2(false, false);
    return float(source);
}

// run: test_bvec2_to_float_all_false() ~= 0.0

float test_bvec2_to_float_variable() {
    bvec2 x = bvec2(true, false);
    return float(x);
}

// run: test_bvec2_to_float_variable() ~= 1.0

float test_bvec2_to_float_expression() {
    return float(not(bvec2(false, true)));
}

// run: test_bvec2_to_float_expression() ~= 1.0

float test_bvec2_to_float_in_arithmetic() {
    bvec2 x = bvec2(true, false);
    return float(x) + 2.5;
}

// run: test_bvec2_to_float_in_arithmetic() ~= 3.5
