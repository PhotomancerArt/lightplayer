// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: uvec4 + uvec4 -> uvec4 (component-wise)
// ============================================================================

uvec4 test_uvec4_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    return a + b;
}

// run: test_uvec4_add_positive_positive() == uvec4(7u, 7u, 3u, 4u)

uvec4 test_uvec4_add_zero() {
    uvec4 a = uvec4(42u, 17u, 23u, 8u);
    uvec4 b = uvec4(0u, 0u, 0u, 0u);
    return a + b;
}

// run: test_uvec4_add_zero() == uvec4(42u, 17u, 23u, 8u)

uvec4 test_uvec4_add_variables() {
    uvec4 a = uvec4(15u, 10u, 5u, 12u);
    uvec4 b = uvec4(27u, 5u, 12u, 3u);
    return a + b;
}

// run: test_uvec4_add_variables() == uvec4(42u, 15u, 17u, 15u)

uvec4 test_uvec4_add_expressions() {
    return uvec4(8u, 4u, 6u, 2u) + uvec4(6u, 2u, 3u, 4u);
}

// run: test_uvec4_add_expressions() == uvec4(14u, 6u, 9u, 6u)

uvec4 test_uvec4_add_in_assignment() {
    uvec4 result = uvec4(5u, 3u, 2u, 1u);
    result = result + uvec4(10u, 7u, 8u, 9u);
    return result;
}

// run: test_uvec4_add_in_assignment() == uvec4(15u, 10u, 10u, 10u)

uvec4 test_uvec4_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    uvec4 a = uvec4(100000u, 50000u, 25000u, 10000u);
    uvec4 b = uvec4(200000u, 30000u, 15000u, 5000u);
    return a + b;
}

// run: test_uvec4_add_large_numbers() == uvec4(300000u, 80000u, 40000u, 15000u)

uvec4 test_uvec4_add_max_values() {
    uvec4 a = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    uvec4 b = uvec4(1u, 1u, 1u, 1u);
    return a + b;
}

// run: test_uvec4_add_max_values() == uvec4(0u, 4294967295u, 4294967294u, 4294967293u)

uvec4 test_uvec4_add_mixed_components() {
    uvec4 a = uvec4(100u, 50u, 75u, 25u);
    uvec4 b = uvec4(200u, 75u, 150u, 50u);
    return a + b;
}

// run: test_uvec4_add_mixed_components() == uvec4(300u, 125u, 225u, 75u)

