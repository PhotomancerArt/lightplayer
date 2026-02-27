// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Local const and literal expression as array size.

int test_constant_variable() {
    const int n = 5;
    int arr[n];
    arr[0] = 10;
    arr[4] = 50;
    return arr[0] + arr[4];
}

// run: test_constant_variable() == 60 [expect-fail]

int test_constant_expression() {
    int arr[3 + 2];
    arr[0] = 1;
    arr[4] = 5;
    return arr[0] + arr[4];
}

// run: test_constant_expression() == 6 [expect-fail]

int test_multiple_constants() {
    const int a = 2;
    const int b = 3;
    int arr[a * b];
    arr[0] = 100;
    arr[5] = 600;
    return arr[0] + arr[5];
}

// run: test_multiple_constants() == 700 [expect-fail]
