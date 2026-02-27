// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Binary operators on const (+, -, *, /, %).

const float PI = 3.14159;
const float TWO_PI = 2.0 * PI;
const float PI_OVER_TWO = PI / 2.0;
const int ANSWER = 42;
const int DOUBLE_ANSWER = ANSWER * 2;

float test_operators_arithmetic() {
    return TWO_PI + PI_OVER_TWO;
}

// run: test_operators_arithmetic() ~= 7.85398

int test_operators_int_math() {
    return DOUBLE_ANSWER / 2;
}

// run: test_operators_int_math() == 42
