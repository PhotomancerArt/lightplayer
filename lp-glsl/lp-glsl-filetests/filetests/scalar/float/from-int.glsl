// test run
// target riscv32.q32

// ============================================================================
// Constructor: float(int) - converts int to float
// ============================================================================

float test_float_from_int_zero() {
    int i = 0;
    return float(i);
}

// run: test_float_from_int_zero() ~= 0.0

float test_float_from_int_positive() {
    int i = 42;
    return float(i);
}

// run: test_float_from_int_positive() ~= 42.0

float test_float_from_int_negative() {
    int i = -10;
    return float(i);
}

// run: test_float_from_int_negative() ~= -10.0

float test_float_from_int_one() {
    int i = 1;
    return float(i);
}

// run: test_float_from_int_one() ~= 1.0

float test_float_from_int_literal_zero() {
    return float(0);
}

// run: test_float_from_int_literal_zero() ~= 0.0

float test_float_from_int_literal_positive() {
    return float(100);
}

// run: test_float_from_int_literal_positive() ~= 100.0

float test_float_from_int_literal_negative() {
    return float(-50);
}

// run: test_float_from_int_literal_negative() ~= -50.0

float test_float_from_int_expression() {
    int a = 5;
    int b = 3;
    return float(a - b);
}

// run: test_float_from_int_expression() ~= 2.0

float test_float_from_int_expression_negative() {
    int a = 3;
    int b = 5;
    return float(a - b);
}

// run: test_float_from_int_expression_negative() ~= -2.0

float test_float_from_int_large() {
    int i = 2147483647;  // INT_MAX - clamped to fixed16x16 max
    return float(i);
}

// run: test_float_from_int_large() ~= 32767.0

float test_float_from_int_min() {
    int i = -2147483648;  // INT_MIN - clamped to fixed16x16 min
    return float(i);
}

// run: test_float_from_int_min() ~= -32768.0

