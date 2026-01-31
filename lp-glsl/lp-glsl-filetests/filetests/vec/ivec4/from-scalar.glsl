// test run
// target riscv32.q32

// ============================================================================
// From Scalar: ivec4(int) - broadcast single int to all components
// ============================================================================

ivec4 test_ivec4_from_scalar_positive() {
    // Constructor ivec4(int) broadcasts single int to all components
    return ivec4(5);
}

// run: test_ivec4_from_scalar_positive() == ivec4(5, 5, 5, 5)

ivec4 test_ivec4_from_scalar_negative() {
    return ivec4(-3);
}

// run: test_ivec4_from_scalar_negative() == ivec4(-3, -3, -3, -3)

ivec4 test_ivec4_from_scalar_zero() {
    return ivec4(0);
}

// run: test_ivec4_from_scalar_zero() == ivec4(0, 0, 0, 0)

ivec4 test_ivec4_from_scalar_variable() {
    int x = 42;
    return ivec4(x);
}

// run: test_ivec4_from_scalar_variable() == ivec4(42, 42, 42, 42)

ivec4 test_ivec4_from_scalar_expression() {
    return ivec4(10 - 5);
}

// run: test_ivec4_from_scalar_expression() == ivec4(5, 5, 5, 5)

ivec4 test_ivec4_from_scalar_function_result() {
    return ivec4(int(7.8)); // truncates float to int
}

// run: test_ivec4_from_scalar_function_result() == ivec4(7, 7, 7, 7)

ivec4 test_ivec4_from_scalar_in_assignment() {
    ivec4 result;
    result = ivec4(-8);
    return result;
}

// run: test_ivec4_from_scalar_in_assignment() == ivec4(-8, -8, -8, -8)

ivec4 test_ivec4_from_scalar_large_value() {
    return ivec4(100000);
}

// run: test_ivec4_from_scalar_large_value() == ivec4(100000, 100000, 100000, 100000)

ivec4 test_ivec4_from_scalar_computation() {
    return ivec4(2 * 3 + 1);
}

// run: test_ivec4_from_scalar_computation() == ivec4(7, 7, 7, 7)
