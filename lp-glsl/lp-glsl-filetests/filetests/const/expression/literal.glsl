// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Literal values as const initializers.

const float PI = 3.14159;
const int ANSWER = 42;
const uint U = 123u;
const bool FLAG = true;
const vec2 V2 = vec2(1.0, 0.0);
const vec3 V3 = vec3(0.0, 1.0, 0.0);
const vec4 V4 = vec4(0.0, 0.0, 0.0, 1.0);
const mat2 M2 = mat2(1.0, 0.0, 0.0, 1.0);

float test_literal_float() {
    return PI;
}

// run: test_literal_float() ~= 3.14159
int test_literal_int() {
    return ANSWER;
}

// run: test_literal_int() == 42
vec2 test_literal_vec2() {
    return V2;
}

// run: test_literal_vec2() ~= vec2(1.0, 0.0)
mat2 test_literal_mat2() {
    return M2;
}

// run: test_literal_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0)