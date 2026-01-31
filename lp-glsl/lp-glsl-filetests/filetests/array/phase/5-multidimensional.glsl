// test run
// target riscv32.q32

// Phase 5: Multi-dimensional Arrays - Nested arrays with multi-dimensional indexing

// Test 1: 2D array declaration and basic structure
int test_2d_array_declaration() {
    int arr[2][3];
    // Just verify it compiles and can be declared
    return 42; // Placeholder return value
}
// run: test_2d_array_declaration() == 42 [expect-fail]

// Test 2: 2D array element assignment
int test_2d_array_assignment() {
    int arr[2][2];
    arr[0][0] = 10;
    arr[0][1] = 20;
    arr[1][0] = 30;
    arr[1][1] = 40;
    return arr[0][0]; // Should return 10
}
// run: test_2d_array_assignment() == 10 [expect-fail]

// Test 3: 2D array element access - first row
int test_2d_array_access_first_row() {
    int arr[3][2];
    arr[0][0] = 1;
    arr[0][1] = 2;

    int x = arr[0][0];
    int y = arr[0][1];
    return x + y; // Should be 1 + 2 = 3
}
// run: test_2d_array_access_first_row() == 3 [expect-fail]

// Test 4: 2D array element access - second row
int test_2d_array_access_second_row() {
    int arr[3][2];
    arr[1][0] = 3;
    arr[1][1] = 4;

    int x = arr[1][0];
    int y = arr[1][1];
    return x + y; // Should be 3 + 4 = 7
}
// run: test_2d_array_access_second_row() == 7 [expect-fail]

// Test 5: 2D array element access - third row
int test_2d_array_access_third_row() {
    int arr[3][2];
    arr[2][0] = 5;
    arr[2][1] = 6;

    int x = arr[2][0];
    int y = arr[2][1];
    return x + y; // Should be 5 + 6 = 11
}
// run: test_2d_array_access_third_row() == 11 [expect-fail]

// Test 6: 2D array mixed read/write operations
int test_2d_array_mixed_operations() {
    int arr[2][3];

    // Write to all elements
    arr[0][0] = 1; arr[0][1] = 2; arr[0][2] = 3;
    arr[1][0] = 4; arr[1][1] = 5; arr[1][2] = 6;

    // Read and compute
    int sum = arr[0][0] + arr[0][2] + arr[1][1] + arr[1][2];
    return sum; // Should be 1 + 3 + 5 + 6 = 15
}
// run: test_2d_array_mixed_operations() == 15 [expect-fail]

// Phase 5 integration test: Full 2D array operations
int phase5() {
    // 2D array
    int arr[3][2];

    // Initialize
    arr[0][0] = 1;
    arr[0][1] = 2;
    arr[1][0] = 3;
    arr[1][1] = 4;
    arr[2][0] = 5;
    arr[2][1] = 6;

    // Read from 2D array
    int x = arr[0][0]; // 1
    int y = arr[1][1]; // 4
    int z = arr[2][0]; // 5

    return x + y + z; // 1 + 4 + 5 = 10
}
// run: phase5() == 10 [expect-fail]

