// test run
// target riscv32.q32

// ============================================================================
// Multi-dimensional Arrays with Const Variables
// ============================================================================

const int ROWS = 3;
const int COLS = 2;

// 2D array with const sizes
float arr_2d[ROWS][COLS];

// 3D array with const sizes
const int DEPTH = 4;
float arr_3d[ROWS][COLS][DEPTH];

// Mixed literal and const
float arr_mixed[5][COLS];

// Const expressions in multi-dim
const int SIZE_X = 2 + 1;
const int SIZE_Y = 3 * 2;
vec3 arr_expr[SIZE_X][SIZE_Y];

// More complex multi-dim with expressions
const int DIM1 = (4 + 2) / 2;
const int DIM2 = 3;
const int DIM3 = DIM1 + DIM2;
vec4 arr_complex[DIM1][DIM2][DIM3];

// Large multi-dim array
const int LARGE_ROWS = 10;
const int LARGE_COLS = 5;
int arr_large[LARGE_ROWS][LARGE_COLS];

float test_2d_const_sizes() {
    // Test 2D array with const sizes
    return 1.0;
}

// run: test_2d_const_sizes() == 1.0 [expect-fail]

int test_3d_const_sizes() {
    // Test 3D array with const sizes
    return 1;
}

// run: test_3d_const_sizes() == 1 [expect-fail]

float test_mixed_literal_const() {
    // Test mixed literal and const dimensions
    return 1.0;
}

// run: test_mixed_literal_const() == 1.0 [expect-fail]

vec2 test_const_expr_multidim() {
    // Test const expressions in multi-dimensional arrays
    return vec2(1.0, 1.0);
}

// run: test_const_expr_multidim() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_complex_multidim() {
    // Test complex multi-dimensional with const expressions
    return vec3(1.0, 1.0, 1.0);
}

// run: test_complex_multidim() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

vec4 test_large_multidim() {
    // Test large multi-dimensional array with const sizes
    return vec4(1.0, 1.0, 1.0, 1.0);
}

// run: test_large_multidim() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

float test_multidim_different_types() {
    // Different types with const multi-dim arrays
    const int MAT_ROWS = 3;
    const int MAT_COLS = 3;
    mat3 matrices[MAT_ROWS][MAT_COLS];
    return 1.0;
}

// run: test_multidim_different_types() == 1.0 [expect-fail]

int test_multidim_vectors() {
    // Vector arrays with const multi-dim
    const int VEC_ROWS = 2;
    const int VEC_COLS = 4;
    ivec3 int_vecs[VEC_ROWS][VEC_COLS];
    return 1;
}

// run: test_multidim_vectors() == 1 [expect-fail]




