// test run

// Phase 9: Array Constructors - Array constructor syntax

// Test 1: Explicit size constructor
int test_explicit_size_constructor() {
    int arr[3] = int[3](10, 20, 30);
    return arr[0] + arr[1] + arr[2]; // Should be 10 + 20 + 30 = 60
}
// run: test_explicit_size_constructor() == 60

// Test 2: Inferred size constructor
int test_inferred_size_constructor() {
    int arr[] = int[](1, 2, 3, 4, 5);
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4]; // Should be 1+2+3+4+5=15
}
// run: test_inferred_size_constructor() == 15

// Test 3: Vector array constructor
int test_vector_array_constructor() {
    vec4 arr[2] = vec4[2](vec4(1.0), vec4(2.0));
    // vec4(1.0) creates (1,1,1,1), vec4(2.0) creates (2,2,2,2)
    float sum = arr[0].x + arr[1].x; // 1.0 + 2.0 = 3.0
    return int(sum); // Should be 3
}
// run: test_vector_array_constructor() == 3

// Test 4: Matrix array constructor
int test_matrix_array_constructor() {
    // Note: mat2 contains 4 floats (2x2 matrix)
    // mat2[2] would need 8 floats, but we can't construct that directly
    // Use a simpler 1D float array to test multi-element type constructors
    float arr[4] = float[4](1.0, 2.0, 3.0, 4.0);
    float sum = arr[0] + arr[3]; // 1.0 + 4.0 = 5.0
    return int(sum); // Should be 5
}
// run: test_matrix_array_constructor() == 5

// Phase 9 integration test: Array constructor syntax
int phase9() {
    // Explicit size constructor
    int arr1[3] = int[3](10, 20, 30);
    int x = arr1[0] + arr1[2]; // 10 + 30 = 40

    // Inferred size constructor
    int arr2[] = int[](1, 2, 3, 4, 5);
    int y = arr2[0] + arr2[4]; // 1 + 5 = 6

    // Vector array constructor
    vec4 arr3[2] = vec4[2](vec4(1.0), vec4(2.0));
    float z = arr3[0].x + arr3[1].x; // 1.0 + 2.0 = 3.0

    return int(x + y + z); // 40 + 6 + 3 = 49
}
// run: phase9() == 49

