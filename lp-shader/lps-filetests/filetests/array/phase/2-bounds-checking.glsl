// test run

// Phase 2: Bounds Checking - Runtime bounds clamping for array reads and writes
// Note: v1 uses clamping (OOB accesses clamp to valid range). Future versions may trap.

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

// Test 5: Bounds clamping - negative index read (clamps to 0)
int test_bounds_negative_index_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: negative index clamps to 0
    int i=-1;
    int result = arr[i]; // Reads arr[0] due to clamping
    return result; // Should return 1
}
// run: test_bounds_negative_index_read() == 1

// Test 6: Bounds clamping - upper bound read (clamps to last element)
int test_bounds_upper_bound_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: index 3 clamps to 2 (last element)
    int i=3;
    int result = arr[i]; // Reads arr[2] due to clamping
    return result; // Should return 3
}
// run: test_bounds_upper_bound_read() == 3

// Test 7: Bounds clamping - large out-of-bounds read (clamps to last element)
int test_bounds_large_index_read() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: large index clamps to 2 (last element)
    int i=100;
    int result = arr[i]; // Reads arr[2] due to clamping
    return result; // Should return 3
}
// run: test_bounds_large_index_read() == 3

// Test 8: Bounds clamping - negative index write (clamps to 0)
int test_bounds_negative_index_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: negative index clamps to 0
    int i=-1;
    arr[i] = 999; // Writes to arr[0] due to clamping
    return arr[0]; // Should return 999
}
// run: test_bounds_negative_index_write() == 999

// Test 9: Bounds clamping - upper bound write (clamps to last element)
int test_bounds_upper_bound_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: index 3 clamps to 2 (last element)
    int i=3;
    arr[i] = 999; // Writes to arr[2] due to clamping
    return arr[2]; // Should return 999
}
// run: test_bounds_upper_bound_write() == 999

// Test 10: Bounds clamping - large out-of-bounds write (clamps to last element)
int test_bounds_large_index_write() {
    int arr[3];
    arr[0] = 1;
    arr[1] = 2;
    arr[2] = 3;

    // Out-of-bounds: large index clamps to 2 (last element)
    int i=100;
    arr[i] = 999; // Writes to arr[2] due to clamping
    return arr[2]; // Should return 999
}
// run: test_bounds_large_index_write() == 999

// Phase 2 integration test: Full bounds clamping verification
int phase2() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;

    // Valid access
    int x = arr[0];  // 10
    int y = arr[2];  // 30

    // OOB accesses clamp to valid range:
    int neg = arr[-1];     // Clamps to arr[0] = 10
    int oob = arr[5];      // Clamps to arr[2] = 30

    return x + y + neg + oob; // 10 + 30 + 10 + 30 = 80
}
// run: phase2() == 80

