// test run
// target riscv32.q32

// ============================================================================
// From Scalars: uvec3(uint, uint, uint) - from 3 uint values
// ============================================================================

uvec3 test_uvec3_from_scalars_positive_positive_positive() {
    // Constructor uvec3(uint, uint, uint) from three uint values
    return uvec3(5u, 3u, 7u);
}

// run: test_uvec3_from_scalars_positive_positive_positive() == uvec3(5u, 3u, 7u)

uvec3 test_uvec3_from_scalars_positive_positive_zero() {
    return uvec3(10u, 8u, 0u);
}

// run: test_uvec3_from_scalars_positive_positive_zero() == uvec3(10u, 8u, 0u)

uvec3 test_uvec3_from_scalars_zero_positive_positive() {
    return uvec3(0u, 7u, 12u);
}

// run: test_uvec3_from_scalars_zero_positive_positive() == uvec3(0u, 7u, 12u)

uvec3 test_uvec3_from_scalars_positive_zero_positive() {
    return uvec3(15u, 0u, 9u);
}

// run: test_uvec3_from_scalars_positive_zero_positive() == uvec3(15u, 0u, 9u)

uvec3 test_uvec3_from_scalars_zero_zero_zero() {
    return uvec3(0u, 0u, 0u);
}

// run: test_uvec3_from_scalars_zero_zero_zero() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_from_scalars_max_values() {
    return uvec3(4294967295u, 4294967295u, 4294967295u);
}

// run: test_uvec3_from_scalars_max_values() == uvec3(4294967295u, 4294967295u, 4294967295u)

uvec3 test_uvec3_from_scalars_mixed_values() {
    return uvec3(123u, 456789u, 999999u);
}

// run: test_uvec3_from_scalars_mixed_values() == uvec3(123u, 456789u, 999999u)

uvec3 test_uvec3_from_scalars_variables() {
    uint x = 42u;
    uint y = 17u;
    uint z = 99u;
    return uvec3(x, y, z);
}

// run: test_uvec3_from_scalars_variables() == uvec3(42u, 17u, 99u)

uvec3 test_uvec3_from_scalars_expressions() {
    return uvec3(10u + 5u, 20u * 2u, 100u / 4u);
}

// run: test_uvec3_from_scalars_expressions() == uvec3(15u, 40u, 25u)

uvec3 test_uvec3_from_scalars_function_results() {
    return uvec3(uint(7.8), uint(-3.2), uint(42.0)); // float to uint conversion (truncates)
}

// run: test_uvec3_from_scalars_function_results() == uvec3(7u, 4294967293u, 42u)

uvec3 test_uvec3_from_scalars_in_assignment() {
    uvec3 result;
    result = uvec3(100u, 200u, 300u);
    return result;
}

// run: test_uvec3_from_scalars_in_assignment() == uvec3(100u, 200u, 300u)
