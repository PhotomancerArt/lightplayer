// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// const int/uint as array size.

const int SIZE = 5;
float arr[SIZE];

const uint U_SIZE = 3u;
int arr2[U_SIZE];

const int COMMON_SIZE = 10;
float arr_a[COMMON_SIZE];
int arr_b[COMMON_SIZE];

float test_const_global_arrays() {
    return 1.0;
}

// run: test_const_global_arrays() == 1.0

float test_local_const() {
    const int LOCAL_SIZE = 4;
    float local_arr[LOCAL_SIZE];
    return 1.0;
}

// run: test_local_const() == 1.0
