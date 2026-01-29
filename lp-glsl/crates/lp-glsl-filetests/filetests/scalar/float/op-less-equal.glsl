// test run
// target riscv32.q32

// ============================================================================
// Less Equal: float <= float -> bool
// ============================================================================

bool test_float_less_equal_simple() {
    // Simple less than or equal comparison
    return 3.0 <= 5.0;
}

// run: test_float_less_equal_simple() == true

bool test_float_less_equal_equal() {
    return 5.0 <= 5.0;
}

// run: test_float_less_equal_equal() == true

bool test_float_less_equal_greater() {
    return 5.0 <= 3.0;
}

// run: test_float_less_equal_greater() == false

bool test_float_less_equal_negative() {
    return (-5.0) <= (-3.0);
}

// run: test_float_less_equal_negative() == true

bool test_float_less_equal_negative_equal() {
    return (-3.14) <= (-3.14);
}

// run: test_float_less_equal_negative_equal() == true

bool test_float_less_equal_mixed_sign() {
    return (-2.0) <= 3.0;
}

// run: test_float_less_equal_mixed_sign() == true

bool test_float_less_equal_variables() {
    float a = 10.5;
    float b = 15.3;
    return a <= b;
}

// run: test_float_less_equal_variables() == true

bool test_float_less_equal_variables_equal() {
    float a = 10.5;
    float b = 10.5;
    return a <= b;
}

// run: test_float_less_equal_variables_equal() == true

bool test_float_less_equal_expressions() {
    return (2.0 + 3.0) <= (6.0 - 1.0);
}

// run: test_float_less_equal_expressions() == true

bool test_float_less_equal_zero() {
    return 0.0 <= 0.0;
}

// run: test_float_less_equal_zero() == true
