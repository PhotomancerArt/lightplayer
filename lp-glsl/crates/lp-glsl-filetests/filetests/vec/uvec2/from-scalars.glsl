// test run
// target riscv32.q32

// ============================================================================
// From Scalars: uvec2(uint, uint) - from 2 uint values
// ============================================================================

uvec2 test_uvec2_from_scalars_positive_positive() {
    // Constructor uvec2(uint, uint) from two uint values
    return uvec2(5u, 3u);
}

// run: test_uvec2_from_scalars_positive_positive() == uvec2(5u, 3u)

uvec2 test_uvec2_from_scalars_positive_zero() {
    return uvec2(10u, 0u);
}

// run: test_uvec2_from_scalars_positive_zero() == uvec2(10u, 0u)

uvec2 test_uvec2_from_scalars_zero_positive() {
    return uvec2(0u, 7u);
}

// run: test_uvec2_from_scalars_zero_positive() == uvec2(0u, 7u)

uvec2 test_uvec2_from_scalars_zero_zero() {
    return uvec2(0u, 0u);
}

// run: test_uvec2_from_scalars_zero_zero() == uvec2(0u, 0u)

uvec2 test_uvec2_from_scalars_max_values() {
    return uvec2(4294967295u, 4294967295u);
}

// run: test_uvec2_from_scalars_max_values() == uvec2(4294967295u, 4294967295u)

uvec2 test_uvec2_from_scalars_mixed_values() {
    return uvec2(123u, 456789u);
}

// run: test_uvec2_from_scalars_mixed_values() == uvec2(123u, 456789u)

uvec2 test_uvec2_from_scalars_variables() {
    uint x = 42u;
    uint y = 17u;
    return uvec2(x, y);
}

// run: test_uvec2_from_scalars_variables() == uvec2(42u, 17u)

uvec2 test_uvec2_from_scalars_expressions() {
    return uvec2(10u + 5u, 20u * 2u);
}

// run: test_uvec2_from_scalars_expressions() == uvec2(15u, 40u)

uvec2 test_uvec2_from_scalars_function_results() {
    return uvec2(uint(7.8), uint(-3.2)); // float to uint conversion (truncates)
}

// run: test_uvec2_from_scalars_function_results() == uvec2(7u, 4294967293u)

uvec2 test_uvec2_from_scalars_in_assignment() {
    uvec2 result;
    result = uvec2(100u, 200u);
    return result;
}

// run: test_uvec2_from_scalars_in_assignment() == uvec2(100u, 200u)
