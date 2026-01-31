// test run
// target riscv32.q32

// ============================================================================
// Const Read-Only: Const global variables are read-only after initialization
// ============================================================================

const float PI = 3.14159;
const int MAX_INT = 2147483647;
const uint MAX_UINT = 4294967295u;
const bool TRUE_CONST = true;
const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const mat2 IDENTITY_MATRIX = mat2(1.0, 0.0, 0.0, 1.0);

float test_const_readonly_float() {
    // Const float is read-only - can only read, not write
    return PI * 2.0;
}

// run: test_const_readonly_float() ~= 6.28318 [expect-fail]

int test_const_readonly_int() {
    // Const int is read-only
    return MAX_INT / 2;
}

// run: test_const_readonly_int() == 1073741823 [expect-fail]

uint test_const_readonly_uint() {
    // Const uint is read-only
    return int(MAX_UINT / 1000000000u);
}

// run: test_const_readonly_uint() == 4 [expect-fail]

bool test_const_readonly_bool() {
    // Const bool is read-only
    return TRUE_CONST;
}

// run: test_const_readonly_bool() == true [expect-fail]

vec2 test_const_readonly_vec2() {
    // Const vec2 is read-only
    return UNIT_VECTOR * 3.0;
}

// run: test_const_readonly_vec2() ~= vec2(3.0, 0.0) [expect-fail]

vec3 test_const_readonly_vec3() {
    // Const vec3 is read-only
    return UP_VECTOR + vec3(0.0, 0.0, 1.0);
}

// run: test_const_readonly_vec3() ~= vec3(0.0, 1.0, 1.0) [expect-fail]

mat2 test_const_readonly_mat2() {
    // Const mat2 is read-only
    return IDENTITY_MATRIX * 2.0;
}

// run: test_const_readonly_mat2() ~= mat2(2.0, 0.0, 0.0, 2.0) [expect-fail]

float test_const_readonly_calculations() {
    // Const variables used in calculations
    float radius = 5.0;
    float circumference = 2.0 * PI * radius;
    float area = PI * radius * radius;

    return circumference + area;
}

// run: test_const_readonly_calculations() ~= 94.2477 [expect-fail]

vec3 test_const_readonly_vector_math() {
    // Const vectors in mathematical operations
    vec3 result = UP_VECTOR;
    result = result + UNIT_VECTOR;
    result = result * 2.0;

    return result;
}

// run: test_const_readonly_vector_math() ~= vec3(2.0, 2.0, 0.0) [expect-fail]

float test_const_readonly_multiple_access() {
    // Multiple accesses to const variables
    float sum = 0.0;
    sum = sum + PI;
    sum = sum + float(MAX_INT) / 1000000000.0;
    sum = sum + float(MAX_UINT) / 1000000000.0;

    return sum;
}

// run: test_const_readonly_multiple_access() ~= 3.14159 [expect-fail]
