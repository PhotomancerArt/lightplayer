// test run

// ============================================================================
// From Scalar: bvec3(bool) - broadcast single bool to all components
// ============================================================================

bvec3 test_bvec3_from_scalar_true() {
    // Constructor bvec3(bool) broadcasts single bool to all components
    return bvec3(true);
}

// run: test_bvec3_from_scalar_true() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalar_false() {
    return bvec3(false);
}

// run: test_bvec3_from_scalar_false() == bvec3(false, false, false)

bvec3 test_bvec3_from_scalar_variable() {
    bool x = true;
    return bvec3(x);
}

// run: test_bvec3_from_scalar_variable() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalar_expression() {
    return bvec3(true && false);
}

// run: test_bvec3_from_scalar_expression() == bvec3(false, false, false)

bvec3 test_bvec3_from_scalar_function_result() {
    return bvec3(any(bvec3(true, false, true)));
}

// run: test_bvec3_from_scalar_function_result() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalar_in_assignment() {
    bvec3 result;
    result = bvec3(false);
    return result;
}

// run: test_bvec3_from_scalar_in_assignment() == bvec3(false, false, false)

// ----------------------------------------------------------------------------
// Call-argument stack (WASM): bvec3(scalar) must contribute exactly 3 values
// before the next argument (lps-wasm broadcast / multi-arg calls).
// ----------------------------------------------------------------------------

int bvec3_true_count(bvec3 a, bvec3 b) {
    int s = 0;
    if (a.x) s = s + 1;
    if (a.y) s = s + 1;
    if (a.z) s = s + 1;
    if (b.x) s = s + 1;
    if (b.y) s = s + 1;
    if (b.z) s = s + 1;
    return s;
}

int test_bvec3_from_scalar_as_first_call_arg() {
    return bvec3_true_count(bvec3(true), bvec3(true, false, true));
}

// run: test_bvec3_from_scalar_as_first_call_arg() == 5
