// test run
// target riscv32.q32

// ============================================================================
// From Scalar: uvec3(uint) - broadcast single uint to all components
// ============================================================================

uvec3 test_uvec3_from_scalar_positive() {
    // Constructor uvec3(uint) broadcasts single uint to all components
    return uvec3(5u);
}

// run: test_uvec3_from_scalar_positive() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_from_scalar_zero() {
    return uvec3(0u);
}

// run: test_uvec3_from_scalar_zero() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_from_scalar_max() {
    return uvec3(4294967295u);
}

// run: test_uvec3_from_scalar_max() == uvec3(4294967295u, 4294967295u, 4294967295u)

uvec3 test_uvec3_from_scalar_variable() {
    uint x = 42u;
    return uvec3(x);
}

// run: test_uvec3_from_scalar_variable() == uvec3(42u, 42u, 42u)

uvec3 test_uvec3_from_scalar_expression() {
    return uvec3(10u - 5u);
}

// run: test_uvec3_from_scalar_expression() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_from_scalar_function_result() {
    return uvec3(uint(7.8)); // float to uint conversion (truncates)
}

// run: test_uvec3_from_scalar_function_result() == uvec3(7u, 7u, 7u)

uvec3 test_uvec3_from_scalar_in_assignment() {
    uvec3 result;
    result = uvec3(8u);
    return result;
}

// run: test_uvec3_from_scalar_in_assignment() == uvec3(8u, 8u, 8u)

uvec3 test_uvec3_from_scalar_large_value() {
    return uvec3(100000u);
}

// run: test_uvec3_from_scalar_large_value() == uvec3(100000u, 100000u, 100000u)

uvec3 test_uvec3_from_scalar_computation() {
    return uvec3(2u * 3u + 1u);
}

// run: test_uvec3_from_scalar_computation() == uvec3(7u, 7u, 7u)
