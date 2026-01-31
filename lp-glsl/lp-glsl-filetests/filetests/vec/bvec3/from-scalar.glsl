// test run
// target riscv32.q32

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
