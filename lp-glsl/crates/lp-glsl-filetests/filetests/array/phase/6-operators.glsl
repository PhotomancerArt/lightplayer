// test run
// target riscv32.q32

// Phase 6: Verify All Operators - Increment/decrement, compound assignment, binary/unary ops

// Test 1: Post-increment and post-decrement operators
int test_post_increment_decrement() {
    int arr[2] = {10, 20};

    int x = arr[0]++;        // x = 10, arr[0] becomes 11
    int y = arr[1]--;        // y = 20, arr[1] becomes 19

    return x + y + arr[0] + arr[1]; // Should be 10 + 20 + 11 + 19 = 60
}
// run: test_post_increment_decrement() == 60 [expect-fail]

// Test 2: Pre-increment and pre-decrement operators
int test_pre_increment_decrement() {
    int arr[2] = {10, 20};

    int x = ++arr[0];        // arr[0] becomes 11, x = 11
    int y = --arr[1];        // arr[1] becomes 19, y = 19

    return x + y + arr[0] + arr[1]; // Should be 11 + 19 + 11 + 19 = 60
}
// run: test_pre_increment_decrement() == 60 [expect-fail]

// Test 3: Compound assignment operators
int test_compound_assignment() {
    int arr[3] = {10, 20, 30};

    arr[0] += 5;     // arr[0] becomes 15
    arr[1] -= 3;     // arr[1] becomes 17
    arr[2] *= 2;     // arr[2] becomes 60

    return arr[0] + arr[1] + arr[2]; // Should be 15 + 17 + 60 = 92
}
// run: test_compound_assignment() == 92 [expect-fail]

// Test 4: Binary operations on array elements
int test_binary_operations() {
    int arr[3] = {10, 20, 30};

    int x = arr[0] + arr[1];  // 10 + 20 = 30
    int y = arr[2] - arr[0];  // 30 - 10 = 20
    int z = arr[1] * 2;       // 20 * 2 = 40
    int w = arr[2] / 3;       // 30 / 3 = 10

    return x + y + z + w; // Should be 30 + 20 + 40 + 10 = 100
}
// run: test_binary_operations() == 100 [expect-fail]

// Test 5: Unary operations on array elements
int test_unary_operations() {
    int arr[3] = {10, -20, 30};

    int x = -arr[0];          // -10
    int y = +arr[1];          // -20 (no change)
    int z = -arr[2];          // -30

    return x + y + z; // Should be -10 + (-20) + (-30) = -60
}
// run: test_unary_operations() == -60 [expect-fail]

// Test 6: Mixed increment/decrement and assignment
int test_mixed_increment_assignment() {
    int arr[2] = {10, 20};

    arr[0]++;        // arr[0] becomes 11
    ++arr[1];        // arr[1] becomes 21
    arr[0] += 5;     // arr[0] becomes 16
    arr[1] -= 3;     // arr[1] becomes 18

    return arr[0] + arr[1]; // Should be 16 + 18 = 34
}
// run: test_mixed_increment_assignment() == 34 [expect-fail]

// Phase 6 integration test: All operators together
int phase6() {
    int arr[3] = {10, 20, 30};

    // Increment/decrement
    arr[0]++;        // arr[0] becomes 11
    ++arr[1];        // arr[1] becomes 21
    arr[2]--;        // arr[2] becomes 29
    --arr[0];        // arr[0] becomes 10

    // Compound assignment
    arr[0] += 5;     // arr[0] becomes 15
    arr[1] -= 3;     // arr[1] becomes 18
    arr[2] *= 2;     // arr[2] becomes 58

    // Binary operations
    int x = arr[0] + arr[1];  // 15 + 18 = 33
    int y = arr[2] - arr[0];  // 58 - 15 = 43
    int z = arr[0] * 2;       // 15 * 2 = 30

    // Unary operations
    int a = -arr[0];          // -15
    int b = +arr[1];          // 18

    return x + y + z + a + b; // 33 + 43 + 30 - 15 + 18 = 109
}
// run: phase6() == 109 [expect-fail]

