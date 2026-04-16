// test run

// Phase 7: Function Parameters - Arrays as function parameters and return values

// Helper function: Sum all elements of an array
int sum_array(int arr[5]) {
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4];
}

// Helper function: Find maximum element in array
int max_array(int arr[3]) {
    int max_val = arr[0];
    if (arr[1] > max_val) max_val = arr[1];
    if (arr[2] > max_val) max_val = arr[2];
    return max_val;
}

// Helper function: Multiply array elements by 2
int multiply_and_sum(int arr[3]) {
    return (arr[0] * 2) + (arr[1] * 2) + (arr[2] * 2);
}

// Test 1: Basic array parameter passing (actual sizes must match for Naga / GLSL typing)
int test_array_parameter_basic() {
    int arr[5] = {10, 20, 30, 0, 0};
    return sum_array(arr); // 10+20+30+0+0 = 60
}
// run: test_array_parameter_basic() == 60

// Test 2: Array parameter with sum function
int test_array_parameter_sum() {
    int arr[5] = {1, 2, 3, 4, 5};
    int result = sum_array(arr);
    return result; // Should be 1+2+3+4+5=15
}
// run: test_array_parameter_sum() == 15

// Test 3: Array parameter with max function
int test_array_parameter_max() {
    int arr[3] = {5, 9, 3};
    int result = max_array(arr);
    return result; // Should be 9
}
// run: test_array_parameter_max() == 9

// Test 4: Array parameter with computation function
int test_array_parameter_multiply() {
    int arr[3] = {1, 2, 3};
    int result = multiply_and_sum(arr);
    return result; // Should be (1*2) + (2*2) + (3*2) = 2+4+6=12
}
// run: test_array_parameter_multiply() == 12

// Test 5: Multiple function calls with different arrays
int test_multiple_array_function_calls() {
    int arr1[5] = {1, 1, 1, 1, 1};
    int arr2[3] = {10, 20, 30};

    int sum1 = sum_array(arr1);    // 1+1+1+1+1=5
    int max2 = max_array(arr2);    // 30
    int mult2 = multiply_and_sum(arr2); // (10*2)+(20*2)+(30*2) = 120

    return sum1 + max2 + mult2; // 5 + 30 + 120 = 155
}
// run: test_multiple_array_function_calls() == 155

// Phase 7 integration test: Arrays as function parameters and return values
int phase7() {
    int arr[5] = {1, 2, 3, 4, 5};

    // Pass array to function
    int result = sum_array(arr);

    return result; // Should be 15
}
// run: phase7() == 15

// ============================================================================
// out and inout array parameters
// ============================================================================

// Helper: Fill array with incrementing values starting from base
void fill_array_out(out int arr[3], int base) {
    arr[0] = base;
    arr[1] = base + 1;
    arr[2] = base + 2;
}

// Test: out parameter - function writes to array
int test_array_out_parameter() {
    int arr[3]; // Uninitialized
    fill_array_out(arr, 10);
    return arr[0] + arr[1] + arr[2]; // Should be 10 + 11 + 12 = 33
}
// run: test_array_out_parameter() == 33

// Helper: Double each element in place
void double_array_inout(inout int arr[4]) {
    arr[0] *= 2;
    arr[1] *= 2;
    arr[2] *= 2;
    arr[3] *= 2;
}

// Test: inout parameter - function reads and writes array
int test_array_inout_parameter() {
    int arr[4] = {1, 2, 3, 4};
    double_array_inout(arr);
    return arr[0] + arr[1] + arr[2] + arr[3]; // Should be 2+4+6+8 = 20
}
// run: test_array_inout_parameter() == 20

// Helper: Sum array into accumulator (inout) and write differences to deltas (out)
void sum_with_deltas(in int values[3], inout int accumulator, out int deltas[3]) {
    int sum = values[0] + values[1] + values[2];
    deltas[0] = sum - values[0];
    deltas[1] = sum - values[1];
    deltas[2] = sum - values[2];
    accumulator += sum;
}

// Test: Mixed in, out, inout parameters with arrays
int test_array_mixed_parameters() {
    int values[3] = {10, 20, 30};
    int accumulator = 5;
    int deltas[3];
    sum_with_deltas(values, accumulator, deltas);
    // accumulator = 5 + 60 = 65
    // deltas = [50, 40, 30]
    return accumulator + deltas[0] + deltas[1] + deltas[2]; // 65+50+40+30 = 185
}
// run: test_array_mixed_parameters() == 185

// ============================================================================
// Multi-dimensional array parameters
// ============================================================================

// Helper: Sum all elements of 2D array
int sum_2d_array(int arr[2][3]) {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 3; j++) {
            sum += arr[i][j];
        }
    }
    return sum;
}

// Test: 2D array as function parameter
int test_2d_array_parameter() {
    int arr[2][3] = {{1, 2, 3}, {4, 5, 6}};
    return sum_2d_array(arr); // Should be 21
}
// run: test_2d_array_parameter() == 21

// Helper: Transpose 2x2 matrix in place via inout
void transpose_2x2_inout(inout int mat[2][2]) {
    int temp = mat[0][1];
    mat[0][1] = mat[1][0];
    mat[1][0] = temp;
}

// Test: inout with 2D array
int test_2d_array_inout() {
    int mat[2][2] = {{1, 2}, {3, 4}};
    transpose_2x2_inout(mat);
    // mat should now be {{1, 3}, {2, 4}}
    return mat[0][0] + mat[0][1] + mat[1][0] + mat[1][1]; // 1+3+2+4 = 10
}
// run: test_2d_array_inout() == 10

