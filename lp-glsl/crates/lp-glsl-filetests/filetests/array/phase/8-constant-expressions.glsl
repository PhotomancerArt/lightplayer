// test run
// target riscv32.q32

// Phase 8: Constant Expression Array Sizes - Support constant expressions for array sizes

// Test 1: Simple constant variable
int test_constant_variable() {
    const int n = 5;
    int arr[n];
    arr[0] = 10;
    arr[4] = 50;
    return arr[0] + arr[4]; // Should be 10 + 50 = 60
}
// run: test_constant_variable() == 60 [expect-fail]

// Test 2: Constant expression
int test_constant_expression() {
    int arr[3 + 2]; // Size 5
    arr[0] = 1;
    arr[4] = 5;
    return arr[0] + arr[4]; // Should be 1 + 5 = 6
}
// run: test_constant_expression() == 6 [expect-fail]

// Test 3: Multiple constants
int test_multiple_constants() {
    const int a = 2;
    const int b = 3;
    int arr[a * b]; // Size 6
    arr[0] = 100;
    arr[5] = 600;
    return arr[0] + arr[5]; // Should be 100 + 600 = 700
}
// run: test_multiple_constants() == 700 [expect-fail]

// Phase 8 integration test: Constant expression array sizes
int phase8() {
    // Constant variable
    const int n = 5;
    int arr1[n];
    arr1[0] = 10;
    arr1[4] = 50;

    // Constant expression
    int arr2[3 + 2]; // Size 5
    arr2[0] = 1;
    arr2[4] = 5;

    // Multiple constants
    const int a = 2;
    const int b = 3;
    int arr3[a * b]; // Size 6
    arr3[0] = 100;
    arr3[5] = 600;

    return arr1[0] + arr1[4] + arr2[0] + arr2[4] + arr3[0] + arr3[5];
    // 10 + 50 + 1 + 5 + 100 + 600 = 766
}
// run: phase8() == 766 [expect-fail]

