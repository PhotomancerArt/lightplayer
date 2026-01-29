// test run
// target riscv32.q32

// ============================================================================
// From Scalar: uvec4(uint) - broadcast single uint to all components
// ============================================================================

uvec4 test_uvec4_from_scalar_positive() {
    // Constructor uvec4(uint) broadcasts single uint to all components
    return uvec4(5u);
}

// run: test_uvec4_from_scalar_positive() == uvec4(5u, 5u, 5u, 5u)

uvec4 test_uvec4_from_scalar_zero() {
    return uvec4(0u);
}

// run: test_uvec4_from_scalar_zero() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_from_scalar_max() {
    return uvec4(4294967295u);
}

// run: test_uvec4_from_scalar_max() == uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u)

uvec4 test_uvec4_from_scalar_variable() {
    uint x = 42u;
    return uvec4(x);
}

// run: test_uvec4_from_scalar_variable() == uvec4(42u, 42u, 42u, 42u)

uvec4 test_uvec4_from_scalar_expression() {
    return uvec4(10u - 5u);
}

// run: test_uvec4_from_scalar_expression() == uvec4(5u, 5u, 5u, 5u)

uvec4 test_uvec4_from_scalar_function_result() {
    return uvec4(uint(7.8)); // float to uint conversion (truncates)
}

// run: test_uvec4_from_scalar_function_result() == uvec4(7u, 7u, 7u, 7u)

uvec4 test_uvec4_from_scalar_in_assignment() {
    uvec4 result;
    result = uvec4(8u);
    return result;
}

// run: test_uvec4_from_scalar_in_assignment() == uvec4(8u, 8u, 8u, 8u)

uvec4 test_uvec4_from_scalar_large_value() {
    return uvec4(100000u);
}

// run: test_uvec4_from_scalar_large_value() == uvec4(100000u, 100000u, 100000u, 100000u)

uvec4 test_uvec4_from_scalar_computation() {
    return uvec4(2u * 3u + 1u);
}

// run: test_uvec4_from_scalar_computation() == uvec4(7u, 7u, 7u, 7u)
