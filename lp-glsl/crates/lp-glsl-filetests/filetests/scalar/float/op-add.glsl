// test run
// target riscv32.q32

// ============================================================================
// Add: float + float -> float
// ============================================================================

float test_float_add_positive_positive() {
    // Addition with positive numbers
    return 1.5 + 2.5;
}

// run: test_float_add_positive_positive() ~= 4.0

float test_float_add_positive_negative() {
    return 5.0 + (-3.0);
}

// run: test_float_add_positive_negative() ~= 2.0

float test_float_add_negative_negative() {
    return (-2.5) + (-1.5);
}

// run: test_float_add_negative_negative() ~= -4.0

float test_float_add_zero() {
    return 3.14 + 0.0;
}

// run: test_float_add_zero() ~= 3.14

float test_float_add_variables() {
    float a = 10.5;
    float b = 20.3;
    return a + b;
}

// run: test_float_add_variables() ~= 30.8

float test_float_add_expressions() {
    return (2.0 + 3.0) + (4.0 + 5.0);
}

// run: test_float_add_expressions() ~= 14.0

float test_float_add_in_assignment() {
    float result = 1.0;
    result = result + 2.5;
    return result;
}

// run: test_float_add_in_assignment() ~= 3.5

float test_float_add_small_numbers() {
    return 0.1 + 0.2;
}

// run: test_float_add_small_numbers() ~= 0.3

float test_float_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998)
    // Addition saturates to max
    return 1000000.0 + 2000000.0;
}

// run: test_float_add_large_numbers() ~= 32768.0
