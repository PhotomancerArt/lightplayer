// test run
// target riscv32.q32

// ============================================================================
// Access Array: bvec4[0], bvec4[1], bvec4[2], bvec4[3] - array-style indexing
// ============================================================================

bool test_bvec4_access_array_index_0() {
    // Array-style indexing
    bvec4 a = bvec4(true, false, true, false);
    return a[0];
}

// run: test_bvec4_access_array_index_0() == true

bool test_bvec4_access_array_index_1() {
    bvec4 a = bvec4(true, false, true, false);
    return a[1];
}

// run: test_bvec4_access_array_index_1() == false

bool test_bvec4_access_array_index_2() {
    bvec4 a = bvec4(true, false, true, false);
    return a[2];
}

// run: test_bvec4_access_array_index_2() == true

bool test_bvec4_access_array_index_3() {
    bvec4 a = bvec4(true, false, true, false);
    return a[3];
}

// run: test_bvec4_access_array_index_3() == false

bool test_bvec4_access_array_variable_index() {
    // Variable indexing
    bvec4 a = bvec4(true, false, true, false);
    int i = 0;
    return a[i];
}

// run: test_bvec4_access_array_variable_index() == true

bool test_bvec4_access_array_expression_index() {
    bvec4 a = bvec4(true, false, true, false);
    return a[3 - 1];
}

// run: test_bvec4_access_array_expression_index() == true

bool test_bvec4_access_array_computed() {
    bvec4 a = bvec4(false, true, false, true);
    int i = int(any(bvec2(true, false))); // i = 1
    return a[i];
}

// run: test_bvec4_access_array_computed() == true

bool test_bvec4_access_array_in_assignment() {
    bvec4 a = bvec4(true, false, true, false);
    bool result = a[3];
    return result;
}

// run: test_bvec4_access_array_in_assignment() == false

bool test_bvec4_access_array_multiple_components() {
    bvec4 a = bvec4(true, false, true, false);
    bool result = a[0] && !a[1] && a[2] && !a[3];
    return result;
}

// run: test_bvec4_access_array_multiple_components() == true

