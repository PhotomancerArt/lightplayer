// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Unary minus in const initializer.

const int NEG_INT = -42;
const float NEG_FLOAT = -3.14159;
const vec2 NEG_VEC = -vec2(1.0, 2.0);

int test_unary_int() {
    return NEG_INT;
}

// run: test_unary_int() == -42
float test_unary_float() {
    return NEG_FLOAT;
}

// run: test_unary_float() ~= -3.14159
vec2 test_unary_vec() {
    return NEG_VEC;
}

// run: test_unary_vec() ~= vec2(-1.0, -2.0)