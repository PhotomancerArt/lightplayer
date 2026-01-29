// test run
// target riscv32.q32

// ============================================================================
// Const Global Initialization: Global variables with const qualifier initialized
// ============================================================================

const float PI = 3.14159;
const int MAX_VALUE = 1000;
const uint UINT_MAX = 4294967295u;
const bool ENABLED = true;
const vec2 ORIGIN = vec2(0.0, 0.0);
const vec3 UP = vec3(0.0, 1.0, 0.0);
const vec4 BLACK = vec4(0.0, 0.0, 0.0, 1.0);
const mat2 IDENTITY = mat2(1.0, 0.0, 0.0, 1.0);

// Const with expressions
const float TWO_PI = 2.0 * PI;
const float HALF_PI = PI / 2.0;
const int DOUBLE_MAX = MAX_VALUE * 2;
const vec3 RIGHT = vec3(1.0, 0.0, 0.0);
const vec3 FORWARD = vec3(0.0, 0.0, 1.0);

float test_initialize_const_float() {
    // Const global float initialization
    return PI;
}

// run: test_initialize_const_float() ~= 3.14159 [expect-fail]

int test_initialize_const_int() {
    // Const global int initialization
    return MAX_VALUE;
}

// run: test_initialize_const_int() == 1000 [expect-fail]

uint test_initialize_const_uint() {
    // Const global uint initialization
    return int(UINT_MAX / 2u);
}

// run: test_initialize_const_uint() == 2147483647 [expect-fail]

bool test_initialize_const_bool() {
    // Const global bool initialization
    return ENABLED;
}

// run: test_initialize_const_bool() == true [expect-fail]

vec2 test_initialize_const_vec2() {
    // Const global vec2 initialization
    return ORIGIN + vec2(1.0, 1.0);
}

// run: test_initialize_const_vec2() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_initialize_const_vec3() {
    // Const global vec3 initialization
    return UP + RIGHT;
}

// run: test_initialize_const_vec3() ~= vec3(1.0, 1.0, 0.0) [expect-fail]

vec4 test_initialize_const_vec4() {
    // Const global vec4 initialization
    return BLACK;
}

// run: test_initialize_const_vec4() ~= vec4(0.0, 0.0, 0.0, 1.0) [expect-fail]

mat2 test_initialize_const_mat2() {
    // Const global mat2 initialization
    return IDENTITY;
}

// run: test_initialize_const_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0) [expect-fail]

float test_initialize_const_expression() {
    // Const global with constant expressions
    return TWO_PI + HALF_PI;
}

// run: test_initialize_const_expression() ~= 9.42477 [expect-fail]

int test_initialize_const_int_expr() {
    // Const global int with expressions
    return DOUBLE_MAX / 4;
}

// run: test_initialize_const_int_expr() == 500 [expect-fail]

vec3 test_initialize_const_vec_expr() {
    // Const global vectors with expressions
    return UP * 2.0 + RIGHT * 3.0 + FORWARD;
}

// run: test_initialize_const_vec_expr() ~= vec3(3.0, 2.0, 1.0) [expect-fail]
