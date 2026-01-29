// test run
// target riscv32.q32

// Phase 3: Initialization - Array initializer lists, full/partial initialization, unsized arrays

// Test 1: Full initialization with explicit size
int test_full_initialization() {
    int arr[3] = {10, 20, 30};
    return arr[0] + arr[1] + arr[2]; // Should be 10 + 20 + 30 = 60
}
// run: test_full_initialization() == 60 [expect-fail]

// Test 2: Partial initialization (remaining elements should be zero)
int test_partial_initialization() {
    int arr[5] = {1, 2, 3};
    // arr[3] and arr[4] should be 0
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4]; // Should be 1 + 2 + 3 + 0 + 0 = 6
}
// run: test_partial_initialization() == 6 [expect-fail]

// Test 3: Unsized array with inferred size from initializer
int test_unsized_array() {
    int arr[] = {100, 200, 300};
    return arr[0] + arr[1] + arr[2]; // Should be 100 + 200 + 300 = 600
}
// run: test_unsized_array() == 600 [expect-fail]

// Test 4: Partial initialization edge case (only first element)
int test_single_element_initialization() {
    int arr[3] = {42};
    // arr[1] and arr[2] should be 0
    return arr[0] + arr[1] + arr[2]; // Should be 42 + 0 + 0 = 42
}
// run: test_single_element_initialization() == 42 [expect-fail]

// Phase 3 integration test: All initialization types together
int phase3() {
    // Full initialization
    int arr1[3] = {10, 20, 30};
    int x = arr1[0] + arr1[1] + arr1[2]; // 60

    // Partial initialization (remaining should be zero)
    int arr2[5] = {1, 2, 3};
    int y = arr2[0] + arr2[4]; // 1 + 0 = 1

    // Unsized array with inferred size from initializer
    int arr3[] = {100, 200, 300};
    int z = arr3[0] + arr3[2]; // 100 + 300 = 400

    return x + y + z; // 60 + 1 + 400 = 461
}
// run: phase3() == 461 [expect-fail]

