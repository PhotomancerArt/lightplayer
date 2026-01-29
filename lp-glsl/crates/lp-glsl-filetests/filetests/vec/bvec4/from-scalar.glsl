// test run
// target riscv32.q32

// ============================================================================
// From Scalar: bvec4(bool) - broadcast single bool to all components
// ============================================================================

bvec4 test_bvec4_from_scalar_true() {
    // Constructor bvec4(bool) broadcasts single bool to all components
    return bvec4(true);
}

// run: test_bvec4_from_scalar_true() == bvec4(true, true, true, true)

bvec4 test_bvec4_from_scalar_false() {
    return bvec4(false);
}

// run: test_bvec4_from_scalar_false() == bvec4(false, false, false, false)

bvec4 test_bvec4_from_scalar_variable() {
    bool x = true;
    return bvec4(x);
}

// run: test_bvec4_from_scalar_variable() == bvec4(true, true, true, true)

bvec4 test_bvec4_from_scalar_expression() {
    return bvec4(true && false);
}

// run: test_bvec4_from_scalar_expression() == bvec4(false, false, false, false)

bvec4 test_bvec4_from_scalar_function_result() {
    return bvec4(any(bvec4(true, false, true, false)));
}

// run: test_bvec4_from_scalar_function_result() == bvec4(true, true, true, true)

bvec4 test_bvec4_from_scalar_in_assignment() {
    bvec4 result;
    result = bvec4(false);
    return result;
}

// run: test_bvec4_from_scalar_in_assignment() == bvec4(false, false, false, false)
