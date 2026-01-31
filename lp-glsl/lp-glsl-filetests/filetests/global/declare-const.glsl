// test run
// target riscv32.q32

// ============================================================================
// Const Global Declarations: Global variables with const qualifier
// ============================================================================

const float PI = 3.14159;
const int MAX_INT = 2147483647;
const uint MAX_UINT = 4294967295u;
const bool TRUE_CONST = true;
const vec2 UNIT_X = vec2(1.0, 0.0);
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const vec4 WHITE = vec4(1.0, 1.0, 1.0, 1.0);
const mat2 IDENTITY_2D = mat2(1.0, 0.0, 0.0, 1.0);

float test_declare_const_float() {
    // Const global float declaration
    return PI * 2.0;
}

// run: test_declare_const_float() ~= 6.28318 [expect-fail]

int test_declare_const_int() {
    // Const global int declaration
    return MAX_INT / 2;
}

// run: test_declare_const_int() == 1073741823 [expect-fail]

uint test_declare_const_uint() {
    // Const global uint declaration
    return int(MAX_UINT / 2u);
}

// run: test_declare_const_uint() == 2147483647 [expect-fail]

bool test_declare_const_bool() {
    // Const global bool declaration
    return TRUE_CONST;
}

// run: test_declare_const_bool() == true [expect-fail]

vec2 test_declare_const_vec2() {
    // Const global vec2 declaration
    return UNIT_X * 2.0;
}

// run: test_declare_const_vec2() ~= vec2(2.0, 0.0) [expect-fail]

vec3 test_declare_const_vec3() {
    // Const global vec3 declaration
    return UP_VECTOR + vec3(0.0, 0.0, 1.0);
}

// run: test_declare_const_vec3() ~= vec3(0.0, 1.0, 1.0) [expect-fail]

vec4 test_declare_const_vec4() {
    // Const global vec4 declaration
    return WHITE * 0.5;
}

// run: test_declare_const_vec4() ~= vec4(0.5, 0.5, 0.5, 0.5) [expect-fail]

mat2 test_declare_const_mat2() {
    // Const global mat2 declaration
    return IDENTITY_2D * 2.0;
}

// run: test_declare_const_mat2() ~= mat2(2.0, 0.0, 0.0, 2.0) [expect-fail]

float test_declare_const_calculated() {
    // Const globals used in calculations
    const float RADIUS = 5.0;
    const float CIRCUMFERENCE = 2.0 * PI * RADIUS;

    return CIRCUMFERENCE;
}

// run: test_declare_const_calculated() ~= 31.4159 [expect-fail]
