// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Const in multi-dimensional array dimensions.

const int ROWS = 3;
const int COLS = 2;
float arr_2d[ROWS][COLS];

const int DEPTH = 4;
float arr_3d[ROWS][COLS][DEPTH];

const int SIZE_X = 2 + 1;
const int SIZE_Y = 3 * 2;
vec3 arr_expr[SIZE_X][SIZE_Y];

float test_2d_const_sizes() {
    return 1.0;
}

// run: test_2d_const_sizes() == 1.0
