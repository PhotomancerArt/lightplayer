// test run
// target riscv32.q32

// ============================================================================
// Const Must Initialize: Const global variables must be initialized at declaration
// ============================================================================

// Valid const declarations with initialization
const float PI = 3.14159;
const int ANSWER = 42;
const uint UINT_CONST = 123u;
const bool FLAG = true;
const vec2 VECTOR_CONST = vec2(1.0, 2.0);
const vec3 COLOR_CONST = vec3(0.5, 0.5, 0.5);
const mat2 MATRIX_CONST = mat2(1.0, 0.0, 0.0, 1.0);

// These would be compile errors if uncommented:
// const float uninit_float;        // Error: const must be initialized
// const int uninit_int;            // Error: const must be initialized
// const vec3 uninit_vec;           // Error: const must be initialized

float test_const_must_init_float() {
    // Const float that is properly initialized
    return PI;
}

// run: test_const_must_init_float() ~= 3.14159 [expect-fail]

int test_const_must_init_int() {
    // Const int that is properly initialized
    return ANSWER;
}

// run: test_const_must_init_int() == 42 [expect-fail]

uint test_const_must_init_uint() {
    // Const uint that is properly initialized
    return int(UINT_CONST);
}

// run: test_const_must_init_uint() == 123 [expect-fail]

bool test_const_must_init_bool() {
    // Const bool that is properly initialized
    return FLAG;
}

// run: test_const_must_init_bool() == true [expect-fail]

vec2 test_const_must_init_vec2() {
    // Const vec2 that is properly initialized
    return VECTOR_CONST;
}

// run: test_const_must_init_vec2() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_const_must_init_vec3() {
    // Const vec3 that is properly initialized
    return COLOR_CONST;
}

// run: test_const_must_init_vec3() ~= vec3(0.5, 0.5, 0.5) [expect-fail]

mat2 test_const_must_init_mat2() {
    // Const mat2 that is properly initialized
    return MATRIX_CONST;
}

// run: test_const_must_init_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0) [expect-fail]

float test_const_must_init_expressions() {
    // Const variables with complex expressions
    const float TWO_PI = 2.0 * PI;
    const float HALF_ANSWER = float(ANSWER) / 2.0;
    const float VECTOR_LENGTH = length(VECTOR_CONST);

    return TWO_PI + HALF_ANSWER + VECTOR_LENGTH;
}

// run: test_const_must_init_expressions() ~= 12.6971 [expect-fail]

vec3 test_const_must_init_vector_expr() {
    // Const vectors with expressions
    const vec3 DOUBLED_COLOR = COLOR_CONST * 2.0;
    const vec3 OFFSET_COLOR = COLOR_CONST + vec3(0.1, 0.1, 0.1);

    return DOUBLED_COLOR + OFFSET_COLOR;
}

// run: test_const_must_init_vector_expr() ~= vec3(1.7, 1.7, 1.7) [expect-fail]
