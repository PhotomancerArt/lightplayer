// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Global const declaration and use.

const float PI = 3.14159;
const int MAX_INT = 2147483647;
const vec2 UNIT_X = vec2(1.0, 0.0);
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const mat2 IDENTITY_2D = mat2(1.0, 0.0, 0.0, 1.0);

float test_global_float() {
    return PI * 2.0;
}

// run: test_global_float() ~= 6.28318
vec2 test_global_vec2() {
    return UNIT_X * 2.0;
}

// run: test_global_vec2() ~= vec2(2.0, 0.0)
vec3 test_global_vec3() {
    return UP_VECTOR + vec3(0.0, 0.0, 1.0);
}

// run: test_global_vec3() ~= vec3(0.0, 1.0, 1.0)