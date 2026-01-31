// test run
// target riscv32.q32

// ============================================================================
// From Scalars: bvec3(bool, bool, bool) - from 3 bool values
// ============================================================================

bvec3 test_bvec3_from_scalars_true_true_true() {
    // Constructor bvec3(bool, bool, bool) from three bool values
    return bvec3(true, true, true);
}

// run: test_bvec3_from_scalars_true_true_true() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalars_true_true_false() {
    return bvec3(true, true, false);
}

// run: test_bvec3_from_scalars_true_true_false() == bvec3(true, true, false)

bvec3 test_bvec3_from_scalars_true_false_true() {
    return bvec3(true, false, true);
}

// run: test_bvec3_from_scalars_true_false_true() == bvec3(true, false, true)

bvec3 test_bvec3_from_scalars_false_true_true() {
    return bvec3(false, true, true);
}

// run: test_bvec3_from_scalars_false_true_true() == bvec3(false, true, true)

bvec3 test_bvec3_from_scalars_false_false_false() {
    return bvec3(false, false, false);
}

// run: test_bvec3_from_scalars_false_false_false() == bvec3(false, false, false)

bvec3 test_bvec3_from_scalars_variables() {
    bool x = true;
    bool y = false;
    bool z = true;
    return bvec3(x, y, z);
}

// run: test_bvec3_from_scalars_variables() == bvec3(true, false, true)

bvec3 test_bvec3_from_scalars_expressions() {
    return bvec3(true && true, false || true, !false);
}

// run: test_bvec3_from_scalars_expressions() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalars_function_results() {
    return bvec3(any(bvec3(true, false, false)), all(bvec3(true, true, true)), any(bvec3(false, false, true)));
}

// run: test_bvec3_from_scalars_function_results() == bvec3(true, true, true)

bvec3 test_bvec3_from_scalars_in_assignment() {
    bvec3 result;
    result = bvec3(false, true, false);
    return result;
}

// run: test_bvec3_from_scalars_in_assignment() == bvec3(false, true, false)
