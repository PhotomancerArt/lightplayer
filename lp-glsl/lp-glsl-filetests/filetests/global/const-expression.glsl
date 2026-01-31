// test run
// target riscv32.q32

// ============================================================================
// Const Expression: Const global initializers must be constant expressions
// ============================================================================

// Valid constant expressions
const float PI = 3.14159;
const float TWO_PI = 2.0 * PI;
const float PI_OVER_TWO = PI / 2.0;
const int ANSWER = 42;
const int DOUBLE_ANSWER = ANSWER * 2;
const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const vec2 SCALED_VECTOR = UNIT_VECTOR * 2.0;
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const vec3 RIGHT_VECTOR = vec3(1.0, 0.0, 0.0);
const vec3 FORWARD_VECTOR = vec3(0.0, 0.0, 1.0);
const mat2 IDENTITY = mat2(1.0, 0.0, 0.0, 1.0);
const mat2 SCALED_IDENTITY = IDENTITY * 2.0;

// These would be compile errors (non-constant expressions):
// float non_const_var = 5.0;
// const float bad_const = non_const_var;  // Error: not constant expression
// const float bad_sin = sin(PI);          // Error: sin() not constant

float test_const_expression_arithmetic() {
    // Constant arithmetic expressions
    return TWO_PI + PI_OVER_TWO;
}

// run: test_const_expression_arithmetic() ~= 9.42477 [expect-fail]

int test_const_expression_int_math() {
    // Constant integer expressions
    return DOUBLE_ANSWER / 2;
}

// run: test_const_expression_int_math() == 42 [expect-fail]

vec2 test_const_expression_vector() {
    // Constant vector expressions
    return SCALED_VECTOR + vec2(1.0, 1.0);
}

// run: test_const_expression_vector() ~= vec2(3.0, 1.0) [expect-fail]

vec3 test_const_expression_vector_ops() {
    // Constant vector operations
    return UP_VECTOR + RIGHT_VECTOR + FORWARD_VECTOR;
}

// run: test_const_expression_vector_ops() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

mat2 test_const_expression_matrix() {
    // Constant matrix expressions
    return SCALED_IDENTITY;
}

// run: test_const_expression_matrix() ~= mat2(2.0, 0.0, 0.0, 2.0) [expect-fail]

float test_const_expression_nested() {
    // Nested constant expressions
    const float QUARTER_PI = PI_OVER_TWO / 2.0;
    const float EIGHTH_PI = QUARTER_PI / 2.0;

    return QUARTER_PI + EIGHTH_PI;
}

// run: test_const_expression_nested() ~= 1.9635 [expect-fail]

vec3 test_const_expression_complex() {
    // Complex constant vector expressions
    const vec3 BASIS_SUM = UP_VECTOR + RIGHT_VECTOR + FORWARD_VECTOR;
    const vec3 SCALED_BASIS = BASIS_SUM * 0.5;
    const vec3 OFFSET_BASIS = SCALED_BASIS + vec3(0.1, 0.1, 0.1);

    return OFFSET_BASIS;
}

// run: test_const_expression_complex() ~= vec3(0.6, 0.6, 0.6) [expect-fail]

float test_const_expression_builtin() {
    // Built-in functions that are constant (if supported)
    // Note: Not all GLSL implementations support all built-ins in const expressions
    const float LENGTH_UNIT = length(UNIT_VECTOR);  // length of (1,0) = 1

    return LENGTH_UNIT;
}

// run: test_const_expression_builtin() ~= 1.0 [expect-fail]
