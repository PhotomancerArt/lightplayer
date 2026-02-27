// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Writing to const is compile error. This file tests read path (write would error).

const float PI = 3.14159;
const int MAX_VALUE = 1000;
const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const mat2 IDENTITY = mat2(1.0, 0.0, 0.0, 1.0);

float test_edge_const_write_error_read() {
    return PI * 2.0;
}

// run: test_edge_const_write_error_read() ~= 6.28318
int test_edge_const_write_error_int() {
    return MAX_VALUE / 2;
}

// run: test_edge_const_write_error_int() == 500
vec2 test_edge_const_write_error_vec() {
    return UNIT_VECTOR * 3.0;
}

// run: test_edge_const_write_error_vec() ~= vec2(3.0, 0.0)
mat2 test_edge_const_write_error_mat() {
    return IDENTITY * 2.0;
}

// run: test_edge_const_write_error_mat() ~= mat2(2.0, 0.0, 0.0, 2.0)
float test_edge_const_write_error_calculations() {
    float radius = 5.0;
    float circumference = 2.0 * PI * radius;
    vec2 scaled_unit = UNIT_VECTOR * radius;
    return circumference + scaled_unit.x + scaled_unit.y;
}

// run: test_edge_const_write_error_calculations() ~= 36.41586