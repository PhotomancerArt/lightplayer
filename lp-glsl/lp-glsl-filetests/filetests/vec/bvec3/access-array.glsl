// test run
// target riscv32.q32

// ============================================================================
// Access Array: bvec3[0], bvec3[1], bvec3[2] - array-style indexing
// ============================================================================

bool test_bvec3_access_array_index_0() {
    // Array-style indexing
    bvec3 a = bvec3(true, false, true);
    return a[0];
}

// run: test_bvec3_access_array_index_0() == true

bool test_bvec3_access_array_index_1() {
    bvec3 a = bvec3(true, false, true);
    return a[1];
}

// run: test_bvec3_access_array_index_1() == false

bool test_bvec3_access_array_index_2() {
    bvec3 a = bvec3(true, false, true);
    return a[2];
}

// run: test_bvec3_access_array_index_2() == true

bool test_bvec3_access_array_variable_index() {
    // Variable indexing
    bvec3 a = bvec3(true, false, true);
    int i = 0;
    return a[i];
}

// run: test_bvec3_access_array_variable_index() == true

bool test_bvec3_access_array_expression_index() {
    bvec3 a = bvec3(true, false, true);
    return a[2 - 1];
}

// run: test_bvec3_access_array_expression_index() == false

bool test_bvec3_access_array_computed() {
    bvec3 a = bvec3(false, true, false);
    int i = int(any(bvec2(true, false))); // i = 1
    return a[i];
}

// run: test_bvec3_access_array_computed() == true

bool test_bvec3_access_array_in_assignment() {
    bvec3 a = bvec3(true, false, true);
    bool result = a[2];
    return result;
}

// run: test_bvec3_access_array_in_assignment() == true

bool test_bvec3_access_array_multiple_components() {
    bvec3 a = bvec3(true, false, true);
    bool result = a[0] && !a[1] && a[2];
    return result;
}

// run: test_bvec3_access_array_multiple_components() == true

