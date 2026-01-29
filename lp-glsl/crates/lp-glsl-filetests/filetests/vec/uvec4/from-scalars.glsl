// test run
// target riscv32.q32

// ============================================================================
// From Scalars: uvec4(uint, uint, uint, uint) - from 4 uint values
// ============================================================================

uvec4 test_uvec4_from_scalars_positive_positive_positive_positive() {
    // Constructor uvec4(uint, uint, uint, uint) from four uint values
    return uvec4(5u, 3u, 7u, 2u);
}

// run: test_uvec4_from_scalars_positive_positive_positive_positive() == uvec4(5u, 3u, 7u, 2u)

uvec4 test_uvec4_from_scalars_positive_positive_positive_zero() {
    return uvec4(10u, 8u, 6u, 0u);
}

// run: test_uvec4_from_scalars_positive_positive_positive_zero() == uvec4(10u, 8u, 6u, 0u)

uvec4 test_uvec4_from_scalars_zero_positive_positive_positive() {
    return uvec4(0u, 7u, 12u, 9u);
}

// run: test_uvec4_from_scalars_zero_positive_positive_positive() == uvec4(0u, 7u, 12u, 9u)

uvec4 test_uvec4_from_scalars_positive_zero_positive_positive() {
    return uvec4(15u, 0u, 9u, 11u);
}

// run: test_uvec4_from_scalars_positive_zero_positive_positive() == uvec4(15u, 0u, 9u, 11u)

uvec4 test_uvec4_from_scalars_positive_positive_zero_positive() {
    return uvec4(13u, 17u, 0u, 23u);
}

// run: test_uvec4_from_scalars_positive_positive_zero_positive() == uvec4(13u, 17u, 0u, 23u)

uvec4 test_uvec4_from_scalars_zero_zero_zero_zero() {
    return uvec4(0u, 0u, 0u, 0u);
}

// run: test_uvec4_from_scalars_zero_zero_zero_zero() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_from_scalars_max_values() {
    return uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u);
}

// run: test_uvec4_from_scalars_max_values() == uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u)

uvec4 test_uvec4_from_scalars_mixed_values() {
    return uvec4(123u, 456789u, 999999u, 777777u);
}

// run: test_uvec4_from_scalars_mixed_values() == uvec4(123u, 456789u, 999999u, 777777u)

uvec4 test_uvec4_from_scalars_variables() {
    uint x = 42u;
    uint y = 17u;
    uint z = 99u;
    uint w = 55u;
    return uvec4(x, y, z, w);
}

// run: test_uvec4_from_scalars_variables() == uvec4(42u, 17u, 99u, 55u)

uvec4 test_uvec4_from_scalars_expressions() {
    return uvec4(10u + 5u, 20u * 2u, 100u / 4u, 50u - 10u);
}

// run: test_uvec4_from_scalars_expressions() == uvec4(15u, 40u, 25u, 40u)

uvec4 test_uvec4_from_scalars_function_results() {
    return uvec4(uint(7.8), uint(-3.2), uint(42.0), uint(15.9)); // float to uint conversion (truncates)
}

// run: test_uvec4_from_scalars_function_results() == uvec4(7u, 4294967293u, 42u, 15u)

uvec4 test_uvec4_from_scalars_in_assignment() {
    uvec4 result;
    result = uvec4(100u, 200u, 300u, 400u);
    return result;
}

// run: test_uvec4_from_scalars_in_assignment() == uvec4(100u, 200u, 300u, 400u)
