// test run
// target riscv32.q32

// ============================================================================
// Access Array: bvec2[0], bvec2[1] - array-style indexing
// ============================================================================

bool test_bvec2_access_array_index_0() {
    // Array-style indexing
    bvec2 a = bvec2(true, false);
    return a[0];
}

// run: test_bvec2_access_array_index_0() == true

bool test_bvec2_access_array_index_1() {
    bvec2 a = bvec2(true, false);
    return a[1];
}

// run: test_bvec2_access_array_index_1() == false

bool test_bvec2_access_array_variable_index() {
    // Variable indexing
    bvec2 a = bvec2(true, false);
    int i = 0;
    return a[i];
}

// run: test_bvec2_access_array_variable_index() == true

bool test_bvec2_access_array_expression_index() {
    bvec2 a = bvec2(true, false);
    return a[1 - 1];
}

// run: test_bvec2_access_array_expression_index() == true

bool test_bvec2_access_array_computed() {
    bvec2 a = bvec2(false, true);
    int i = int(any(bvec2(true, false))); // i = 1
    return a[i];
}

// run: test_bvec2_access_array_computed() == true

bool test_bvec2_access_array_in_assignment() {
    bvec2 a = bvec2(true, false);
    bool result = a[1];
    return result;
}

// run: test_bvec2_access_array_in_assignment() == false

bool test_bvec2_access_array_both_components() {
    bvec2 a = bvec2(true, false);
    bool result = a[0] && !a[1];
    return result;
}

// run: test_bvec2_access_array_both_components() == true
