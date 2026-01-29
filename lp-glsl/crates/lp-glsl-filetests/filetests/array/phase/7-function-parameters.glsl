// test run
// target riscv32.q32

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

// Test 1: Basic array parameter passing
int test_array_parameter_basic() {
    int arr[3] = {10, 20, 30};
    return sum_array(arr); // Should work (will sum first 3 elements: 10+20+30=60)
}
// run: test_array_parameter_basic() == 60 [expect-fail]

// Test 2: Array parameter with sum function
int test_array_parameter_sum() {
    int arr[5] = {1, 2, 3, 4, 5};
    int result = sum_array(arr);
    return result; // Should be 1+2+3+4+5=15
}
// run: test_array_parameter_sum() == 15 [expect-fail]

// Test 3: Array parameter with max function
int test_array_parameter_max() {
    int arr[3] = {5, 9, 3};
    int result = max_array(arr);
    return result; // Should be 9
}
// run: test_array_parameter_max() == 9 [expect-fail]

// Test 4: Array parameter with computation function
int test_array_parameter_multiply() {
    int arr[3] = {1, 2, 3};
    int result = multiply_and_sum(arr);
    return result; // Should be (1*2) + (2*2) + (3*2) = 2+4+6=12
}
// run: test_array_parameter_multiply() == 12 [expect-fail]

// Test 5: Multiple function calls with different arrays
int test_multiple_array_function_calls() {
    int arr1[5] = {1, 1, 1, 1, 1};
    int arr2[3] = {10, 20, 30};

    int sum1 = sum_array(arr1);    // 1+1+1+1+1=5
    int max2 = max_array(arr2);    // 30
    int mult2 = multiply_and_sum(arr2); // (10*2)+(20*2)+(30*2)=60+40+60=160

    return sum1 + max2 + mult2; // Should be 5 + 30 + 160 = 195
}
// run: test_multiple_array_function_calls() == 195 [expect-fail]

// Phase 7 integration test: Arrays as function parameters and return values
int phase7() {
    int arr[5] = {1, 2, 3, 4, 5};

    // Pass array to function
    int result = sum_array(arr);

    return result; // Should be 15
}
// run: phase7() == 15 [expect-fail]

