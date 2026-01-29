// test run
// target riscv32.q32

// ============================================================================
// From Scalars: bvec2(bool, bool) - from 2 bool values
// ============================================================================

bvec2 test_bvec2_from_scalars_true_true() {
    // Constructor bvec2(bool, bool) from two bool values
    return bvec2(true, true);
}

// run: test_bvec2_from_scalars_true_true() == bvec2(true, true)

bvec2 test_bvec2_from_scalars_true_false() {
    return bvec2(true, false);
}

// run: test_bvec2_from_scalars_true_false() == bvec2(true, false)

bvec2 test_bvec2_from_scalars_false_true() {
    return bvec2(false, true);
}

// run: test_bvec2_from_scalars_false_true() == bvec2(false, true)

bvec2 test_bvec2_from_scalars_false_false() {
    return bvec2(false, false);
}

// run: test_bvec2_from_scalars_false_false() == bvec2(false, false)

bvec2 test_bvec2_from_scalars_variables() {
    bool x = true;
    bool y = false;
    return bvec2(x, y);
}

// run: test_bvec2_from_scalars_variables() == bvec2(true, false)

bvec2 test_bvec2_from_scalars_expressions() {
    return bvec2(true && true, false || true);
}

// run: test_bvec2_from_scalars_expressions() == bvec2(true, true)

bvec2 test_bvec2_from_scalars_function_results() {
    return bvec2(any(bvec2(true, false)), all(bvec2(true, true)));
}

// run: test_bvec2_from_scalars_function_results() == bvec2(true, true)

bvec2 test_bvec2_from_scalars_in_assignment() {
    bvec2 result;
    result = bvec2(false, true);
    return result;
}

// run: test_bvec2_from_scalars_in_assignment() == bvec2(false, true)
