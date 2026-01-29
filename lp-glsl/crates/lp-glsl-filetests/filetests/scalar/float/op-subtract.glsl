// test run
// target riscv32.q32

// ============================================================================
// Subtract: float - float -> float
// ============================================================================

float test_float_subtract_positive_positive() {
    // Subtraction with positive numbers
    return 5.5 - 2.5;
}

// run: test_float_subtract_positive_positive() ~= 3.0

float test_float_subtract_positive_negative() {
    return 5.0 - (-3.0);
}

// run: test_float_subtract_positive_negative() ~= 8.0

float test_float_subtract_negative_negative() {
    return (-2.5) - (-1.5);
}

// run: test_float_subtract_negative_negative() ~= -1.0

float test_float_subtract_zero() {
    return 3.14 - 0.0;
}

// run: test_float_subtract_zero() ~= 3.14

float test_float_subtract_variables() {
    float a = 20.5;
    float b = 10.3;
    return a - b;
}

// run: test_float_subtract_variables() ~= 10.2

float test_float_subtract_expressions() {
    return (10.0 - 3.0) - (4.0 - 2.0);
}

// run: test_float_subtract_expressions() ~= 5.0

float test_float_subtract_in_assignment() {
    float result = 5.0;
    result = result - 2.5;
    return result;
}

// run: test_float_subtract_in_assignment() ~= 2.5

float test_float_subtract_small_numbers() {
    return 0.3 - 0.1;
}

// run: test_float_subtract_small_numbers() ~= 0.2

float test_float_subtract_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998)
    // Both operands become max, so max - max = 0
    return 3000000.0 - 1000000.0;
}

// run: test_float_subtract_large_numbers() ~= 0.0
