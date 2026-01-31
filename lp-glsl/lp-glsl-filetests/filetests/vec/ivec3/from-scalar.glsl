// test run
// target riscv32.q32

// ============================================================================
// From Scalar: ivec3(int) - broadcast single int to all components
// ============================================================================

ivec3 test_ivec3_from_scalar_positive() {
    // Constructor ivec3(int) broadcasts single int to all components
    return ivec3(5);
}

// run: test_ivec3_from_scalar_positive() == ivec3(5, 5, 5)

ivec3 test_ivec3_from_scalar_negative() {
    return ivec3(-3);
}

// run: test_ivec3_from_scalar_negative() == ivec3(-3, -3, -3)

ivec3 test_ivec3_from_scalar_zero() {
    return ivec3(0);
}

// run: test_ivec3_from_scalar_zero() == ivec3(0, 0, 0)

ivec3 test_ivec3_from_scalar_variable() {
    int x = 42;
    return ivec3(x);
}

// run: test_ivec3_from_scalar_variable() == ivec3(42, 42, 42)

ivec3 test_ivec3_from_scalar_expression() {
    return ivec3(10 - 5);
}

// run: test_ivec3_from_scalar_expression() == ivec3(5, 5, 5)

ivec3 test_ivec3_from_scalar_function_result() {
    return ivec3(int(7.8)); // truncates float to int
}

// run: test_ivec3_from_scalar_function_result() == ivec3(7, 7, 7)

ivec3 test_ivec3_from_scalar_in_assignment() {
    ivec3 result;
    result = ivec3(-8);
    return result;
}

// run: test_ivec3_from_scalar_in_assignment() == ivec3(-8, -8, -8)

ivec3 test_ivec3_from_scalar_large_value() {
    return ivec3(100000);
}

// run: test_ivec3_from_scalar_large_value() == ivec3(100000, 100000, 100000)

ivec3 test_ivec3_from_scalar_computation() {
    return ivec3(2 * 3 + 1);
}

// run: test_ivec3_from_scalar_computation() == ivec3(7, 7, 7)
