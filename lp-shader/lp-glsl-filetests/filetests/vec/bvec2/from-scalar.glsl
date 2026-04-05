// test run

// ============================================================================
// From Scalar: bvec2(bool) - broadcast single bool to all components
// ============================================================================

bvec2 test_bvec2_from_scalar_true() {
    // Constructor bvec2(bool) broadcasts single bool to all components
    return bvec2(true);
}

// run: test_bvec2_from_scalar_true() == bvec2(true, true)

bvec2 test_bvec2_from_scalar_false() {
    return bvec2(false);
}

// run: test_bvec2_from_scalar_false() == bvec2(false, false)

bvec2 test_bvec2_from_scalar_variable() {
    bool x = true;
    return bvec2(x);
}

// run: test_bvec2_from_scalar_variable() == bvec2(true, true)

bvec2 test_bvec2_from_scalar_expression() {
    return bvec2(true && false);
}

// run: test_bvec2_from_scalar_expression() == bvec2(false, false)

bvec2 test_bvec2_from_scalar_function_result() {
    return bvec2(any(bvec2(true, false)));
}

// run: test_bvec2_from_scalar_function_result() == bvec2(true, true)

bvec2 test_bvec2_from_scalar_in_assignment() {
    bvec2 result;
    result = bvec2(false);
    return result;
}

// run: test_bvec2_from_scalar_in_assignment() == bvec2(false, false)

// ----------------------------------------------------------------------------
// Call-argument stack (WASM): bvec2(scalar) must contribute exactly 2 values
// before the next argument (lp-glsl-wasm broadcast / multi-arg calls).
// ----------------------------------------------------------------------------

int bvec2_true_count(bvec2 a, bvec2 b) {
    int s = 0;
    if (a.x) s = s + 1;
    if (a.y) s = s + 1;
    if (b.x) s = s + 1;
    if (b.y) s = s + 1;
    return s;
}

int test_bvec2_from_scalar_as_first_call_arg() {
    return bvec2_true_count(bvec2(true), bvec2(true, false));
}

// run: test_bvec2_from_scalar_as_first_call_arg() == 3
