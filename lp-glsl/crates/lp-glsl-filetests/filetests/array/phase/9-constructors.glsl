// test run
// target riscv32.q32

// Phase 9: Array Constructors - Array constructor syntax

// Test 1: Explicit size constructor
int test_explicit_size_constructor() {
    int arr = int[3](10, 20, 30);
    return arr[0] + arr[1] + arr[2]; // Should be 10 + 20 + 30 = 60
}
// run: test_explicit_size_constructor() == 60 [expect-fail]

// Test 2: Inferred size constructor
int test_inferred_size_constructor() {
    int arr = int[](1, 2, 3, 4, 5);
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4]; // Should be 1+2+3+4+5=15
}
// run: test_inferred_size_constructor() == 15 [expect-fail]

// Test 3: Vector array constructor
int test_vector_array_constructor() {
    vec4 arr = vec4[2](vec4(1.0), vec4(2.0));
    float sum = arr[0].x + arr[0].y + arr[1].x + arr[1].y; // 1.0 + 0.0 + 2.0 + 0.0 = 3.0
    return int(sum); // Should be 3
}
// run: test_vector_array_constructor() == 3 [expect-fail]

// Test 4: Matrix array constructor
int test_matrix_array_constructor() {
    mat2 arr = mat2[2](mat2(1.0), mat2(2.0));
    float sum = arr[0][0][0] + arr[1][0][0]; // 1.0 + 2.0 = 3.0
    return int(sum); // Should be 3
}
// run: test_matrix_array_constructor() == 3 [expect-fail]

// Phase 9 integration test: Array constructor syntax
int phase9() {
    // Explicit size constructor
    int arr1 = int[3](10, 20, 30);
    int x = arr1[0] + arr1[2]; // 10 + 30 = 40

    // Inferred size constructor
    int arr2 = int[](1, 2, 3, 4, 5);
    int y = arr2[0] + arr2[4]; // 1 + 5 = 6

    // Vector array constructor
    vec4 arr3 = vec4[2](vec4(1.0), vec4(2.0));
    float z = arr3[0].x + arr3[1].x; // 1.0 + 2.0 = 3.0

    return int(x + y + z); // 40 + 6 + 3 = 49
}
// run: phase9() == 49 [expect-fail]

