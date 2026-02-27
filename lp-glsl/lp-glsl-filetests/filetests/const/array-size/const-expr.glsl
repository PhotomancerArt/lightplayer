// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Const expression (2+3, 10-2, etc.) as array size.

const int ADD_SIZE = 2 + 3;
float arr_add[ADD_SIZE];

const int SUB_SIZE = 10 - 2;
int arr_sub[SUB_SIZE];

const int MUL_SIZE = 2 * 3;
float arr_mul[MUL_SIZE];

const int PAREN_SIZE = (2 + 3) * 2;
float arr_paren[PAREN_SIZE];

float test_addition_size() {
    return 1.0;
}

// run: test_addition_size() == 1.0

int test_subtraction_size() {
    return 1;
}

// run: test_subtraction_size() == 1

float test_multiplication_size() {
    return 1.0;
}

// run: test_multiplication_size() == 1.0
