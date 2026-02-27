// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Const variables are read-only after initialization; reading is allowed.

const float PI = 3.14159;
const int MAX_INT = 2147483647;
const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const mat2 IDENTITY_MATRIX = mat2(1.0, 0.0, 0.0, 1.0);

float test_const_readonly_float() {
    return PI * 2.0;
}

// run: test_const_readonly_float() ~= 6.28318 [expect-fail]

int test_const_readonly_int() {
    return MAX_INT / 2;
}

// run: test_const_readonly_int() == 1073741823 [expect-fail]

vec2 test_const_readonly_vec2() {
    return UNIT_VECTOR * 3.0;
}

// run: test_const_readonly_vec2() ~= vec2(3.0, 0.0) [expect-fail]

vec3 test_const_readonly_vec3() {
    return UP_VECTOR + vec3(0.0, 0.0, 1.0);
}

// run: test_const_readonly_vec3() ~= vec3(0.0, 1.0, 1.0) [expect-fail]

mat2 test_const_readonly_mat2() {
    return IDENTITY_MATRIX * 2.0;
}

// run: test_const_readonly_mat2() ~= mat2(2.0, 0.0, 0.0, 2.0) [expect-fail]

float test_const_readonly_calculations() {
    float radius = 5.0;
    float circumference = 2.0 * PI * radius;
    float area = PI * radius * radius;
    return circumference + area;
}

// run: test_const_readonly_calculations() ~= 94.2477 [expect-fail]
