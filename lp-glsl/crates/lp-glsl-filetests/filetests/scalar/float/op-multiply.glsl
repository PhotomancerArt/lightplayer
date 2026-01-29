// test run
// target riscv32.q32

// ============================================================================
// Multiply: float * float -> float
// ============================================================================

float test_float_multiply_positive_positive() {
    // Multiplication with positive numbers
    return 3.0 * 4.0;
}

// run: test_float_multiply_positive_positive() ~= 12.0

float test_float_multiply_positive_negative() {
    return 5.0 * (-2.0);
}

// run: test_float_multiply_positive_negative() ~= -10.0

float test_float_multiply_negative_negative() {
    return (-2.5) * (-2.0);
}

// run: test_float_multiply_negative_negative() ~= 5.0

float test_float_multiply_by_zero() {
    return 3.14 * 0.0;
}

// run: test_float_multiply_by_zero() ~= 0.0

float test_float_multiply_by_one() {
    return 7.5 * 1.0;
}

// run: test_float_multiply_by_one() ~= 7.5

float test_float_multiply_variables() {
    float a = 6.0;
    float b = 7.0;
    return a * b;
}

// run: test_float_multiply_variables() ~= 42.0

float test_float_multiply_expressions() {
    return (2.0 * 3.0) * (4.0 * 5.0);
}

// run: test_float_multiply_expressions() ~= 120.0

float test_float_multiply_in_assignment() {
    float result = 2.0;
    result = result * 3.5;
    return result;
}

// run: test_float_multiply_in_assignment() ~= 7.0

float test_float_multiply_fractions() {
    return 0.5 * 0.25;
}

// run: test_float_multiply_fractions() ~= 0.125

float test_float_multiply_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998)
    // Multiplication saturates to max
    return 1000.0 * 2000.0;
}

// run: test_float_multiply_large_numbers() ~= 32768.0
