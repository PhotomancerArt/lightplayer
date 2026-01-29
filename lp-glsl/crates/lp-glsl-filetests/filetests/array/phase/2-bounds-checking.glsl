// test run
// target riscv32.q32

// Phase 2: Bounds Checking - Runtime bounds checking for array reads and writes

// Test 1: Valid bounds access at index 0
int test_bounds_index_zero() {
    int arr[3];
    arr[0] = 42;
    return arr[0]; // Should return 42
}
// run: test_bounds_index_zero() == 42

// Test 2: Valid bounds access at middle index
int test_bounds_index_middle() {
    int arr[3];
    arr[1] = 100;
    return arr[1]; // Should return 100
}
// run: test_bounds_index_middle() == 100

// Test 3: Valid bounds access at last valid index
int test_bounds_index_last() {
    int arr[3];
    arr[2] = 200;
    return arr[2]; // Should return 200
}
// run: test_bounds_index_last() == 200

// Test 4: Multiple valid bounds accesses
int test_bounds_multiple_access() {
    int arr[5];
    arr[0] = 1;
    arr[2] = 3;
    arr[4] = 5;

    int x = arr[0];
    int y = arr[2];
    int z = arr[4];

    return x + y + z; // Should be 1 + 3 + 5 = 9
}
// run: test_bounds_multiple_access() == 9

// Test 5: Bounds checking failure - negative index read
int test_bounds_negative_index_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime
    int i=-1;
    int bad = arr[i];
    return bad; // Should not reach here
}
// run: test_bounds_negative_index_read() == 0
// EXPECT_TRAP_CODE: 1

// Test 6: Bounds checking failure - upper bound read
int test_bounds_upper_bound_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime (index 3 is out of bounds for array of size 3)
    int i=3;
    int bad = arr[i];
    return bad; // Should not reach here
}
// run: test_bounds_upper_bound_read() == 0
// EXPECT_TRAP_CODE: 1

// Test 7: Bounds checking failure - large out-of-bounds read
int test_bounds_large_index_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime
    int i=100;
    int bad = arr[i];
    return bad; // Should not reach here
}
// run: test_bounds_large_index_read() == 0
// EXPECT_TRAP_CODE: 1

// Test 8: Bounds checking failure - negative index write
int test_bounds_negative_index_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime
    int i=-1;
    arr[i] = 999;
    return arr[0]; // Should not reach here
}
// run: test_bounds_negative_index_write() == 0
// EXPECT_TRAP_CODE: 1

// Test 9: Bounds checking failure - upper bound write
int test_bounds_upper_bound_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime (index 3 is out of bounds for array of size 3)
    int i=3;
    arr[i] = 999;
    return arr[0]; // Should not reach here
}
// run: test_bounds_upper_bound_write() == 0
// EXPECT_TRAP_CODE: 1

// Test 10: Bounds checking failure - large out-of-bounds write
int test_bounds_large_index_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // This should trap at runtime
    int i=100;
    arr[i] = 999;
    return arr[0]; // Should not reach here
}
// run: test_bounds_large_index_write() == 0
// EXPECT_TRAP_CODE: 1

// Phase 2 integration test: Full bounds checking verification
int phase2() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Valid access
    int x = arr[0];
    int y = arr[2];

    // These should trap at runtime (out of bounds):
    // int z = arr[3];  // Would trap
    // arr[-1] = 0;     // Would trap
    // arr[5] = 0;      // Would trap

    return x + y; // Should be 1 + 3 = 4
}
// run: phase2() == 4

