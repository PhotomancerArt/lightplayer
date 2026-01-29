// test run
// target riscv32.q32

// ============================================================================
// Array Size Must Match: Array parameters must have exact size match
// ============================================================================

float sum_array(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_edge_array_size_match() {
    // Array sizes must match exactly
    float[3] data = float[3](1.0, 2.0, 3.0);
    return sum_array(data); // OK: sizes match
}

// run: test_edge_array_size_match() ~= 6.0 [expect-fail]

/*
float test_edge_array_size_mismatch() {
    // Array size mismatch - should be compile error
    // float[2] data = float[2](1.0, 2.0); // Wrong size
    // return sum_array(data); // ERROR: size mismatch

    return 0.0;
}

// run: test_edge_array_size_mismatch() ~= 0.0 [expect-fail]
*/

void process_array(inout int[4] arr) {
    arr[0] = arr[0] * 2;
    arr[1] = arr[1] * 2;
    arr[2] = arr[2] * 2;
    arr[3] = arr[3] * 2;
}

float test_edge_array_size_explicit() {
    // Explicit array sizes must match
    int[4] data = int[4](1, 2, 3, 4);
    process_array(data);
    return float(data[0] + data[1] + data[2] + data[3]);
}

// run: test_edge_array_size_explicit() ~= 20.0 [expect-fail]

/*
float test_edge_array_size_too_small() {
    // Calling with too small array - ERROR
    // float[2] data = float[2](1.0, 2.0); // Too small
    // return sum_three(data); // ERROR

    return 0.0;
}

// run: test_edge_array_size_too_small() ~= 0.0 [expect-fail]
*/

/*
float test_edge_array_size_too_large() {
    // Calling with too large array - ERROR
    // float[3] data = float[3](1.0, 2.0, 3.0); // Too large
    // return sum_two(data); // ERROR

    return 0.0;
}

// run: test_edge_array_size_too_large() ~= 0.0 [expect-fail]
*/

vec2 sum_vectors(vec2[2] arr) {
    return arr[0] + arr[1];
}

float test_edge_array_size_vector() {
    // Vector arrays must match size
    vec2[2] vectors = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    vec2 result = sum_vectors(vectors);
    return result.x + result.y; // 4.0 + 6.0 = 10.0
}

// run: test_edge_array_size_vector() ~= 10.0 [expect-fail]

float sum2(float[2] arr) {
    return arr[0] + arr[1];
}

float sum4(float[4] arr) {
    return arr[0] + arr[1] + arr[2] + arr[3];
}

float test_edge_array_size_different_types() {
    // Different array sizes for different functions
    float[2] arr2 = float[2](1.0, 2.0);
    float[4] arr4 = float[4](1.0, 2.0, 3.0, 4.0);
    return sum2(arr2) + sum4(arr4);
}

// run: test_edge_array_size_different_types() ~= 13.0 [expect-fail]

bool all_true(bool[3] arr) {
    return arr[0] && arr[1] && arr[2];
}

bool test_edge_array_size_bool() {
    // Boolean arrays must match size
    bool[3] flags = bool[3](true, true, true);
    return all_true(flags);
}

// run: test_edge_array_size_bool() == true [expect-fail]

float average(const float[5] arr) {
    return (arr[0] + arr[1] + arr[2] + arr[3] + arr[4]) / 5.0;
}

float test_edge_array_size_const() {
    // Const array parameters must match size
    float[5] data = float[5](10.0, 20.0, 30.0, 40.0, 50.0);
    return average(data);
}

// run: test_edge_array_size_const() ~= 30.0 [expect-fail]

/*
float test_edge_array_size_implicit() {
    // Implicitly sized arrays cannot be used as parameters
    // void process(float[] arr) { } // ERROR: unsized arrays not allowed

    return 0.0;
}

// run: test_edge_array_size_implicit() ~= 0.0 [expect-fail]
*/

float sum_matrix(float[2][3] matrix) {
    return matrix[0][0] + matrix[0][1] + matrix[0][2] +
           matrix[1][0] + matrix[1][1] + matrix[1][2];
}

float test_edge_array_size_multidimensional() {
    // Multidimensional arrays must match all dimensions
    float[2][3] mat = float[2][3](
        float[3](1.0, 2.0, 3.0),
        float[3](4.0, 5.0, 6.0)
    );
    return sum_matrix(mat);
}

// run: test_edge_array_size_multidimensional() ~= 21.0 [expect-fail]
