// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Const variables must be initialized at declaration.

const float PI = 3.14159;
const int ANSWER = 42;
const uint UINT_CONST = 123u;
const bool FLAG = true;
const vec2 VECTOR_CONST = vec2(1.0, 2.0);
const vec3 COLOR_CONST = vec3(0.5, 0.5, 0.5);
const mat2 MATRIX_CONST = mat2(1.0, 0.0, 0.0, 1.0);

float test_const_must_init_float() {
    return PI;
}

// run: test_const_must_init_float() ~= 3.14159
int test_const_must_init_int() {
    return ANSWER;
}

// run: test_const_must_init_int() == 42
uint test_const_must_init_uint() {
    return int(UINT_CONST);
}

// run: test_const_must_init_uint() == 123u
bool test_const_must_init_bool() {
    return FLAG;
}

// run: test_const_must_init_bool() == true
vec2 test_const_must_init_vec2() {
    return VECTOR_CONST;
}

// run: test_const_must_init_vec2() ~= vec2(1.0, 2.0)
vec3 test_const_must_init_vec3() {
    return COLOR_CONST;
}

// run: test_const_must_init_vec3() ~= vec3(0.5, 0.5, 0.5)
mat2 test_const_must_init_mat2() {
    return MATRIX_CONST;
}

// run: test_const_must_init_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0)