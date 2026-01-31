// test run
// target riscv32.q32

// ============================================================================
// Constructor: float(float) - identity constructor
// ============================================================================

float test_float_from_float_zero() {
    float f = 0.0;
    return float(f);
}

// run: test_float_from_float_zero() ~= 0.0

float test_float_from_float_positive() {
    float f = 1.5;
    return float(f);
}

// run: test_float_from_float_positive() ~= 1.5

float test_float_from_float_negative() {
    float f = -2.7;
    return float(f);
}

// run: test_float_from_float_negative() ~= -2.7

float test_float_from_float_literal_zero() {
    return float(0.0);
}

// run: test_float_from_float_literal_zero() ~= 0.0

float test_float_from_float_literal_positive() {
    return float(3.14);
}

// run: test_float_from_float_literal_positive() ~= 3.14

float test_float_from_float_literal_negative() {
    return float(-5.5);
}

// run: test_float_from_float_literal_negative() ~= -5.5

float test_float_from_float_expression() {
    float a = 5.0;
    float b = 2.0;
    return float(a - b);
}

// run: test_float_from_float_expression() ~= 3.0

float test_float_from_float_expression_negative() {
    float a = 3.0;
    float b = 5.0;
    return float(a - b);
}

// run: test_float_from_float_expression_negative() ~= -2.0

float test_float_from_float_nested() {
    float f = 42.5;
    return float(float(f));
}

// run: test_float_from_float_nested() ~= 42.5

float test_float_from_float_self() {
    float a = -100.25;
    float b = float(a);
    return b;
}

// run: test_float_from_float_self() ~= -100.25

