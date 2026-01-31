// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: uvec3 + uvec3 -> uvec3 (component-wise)
// ============================================================================

uvec3 test_uvec3_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    return a + b;
}

// run: test_uvec3_add_positive_positive() == uvec3(7u, 7u, 3u)

uvec3 test_uvec3_add_zero() {
    uvec3 a = uvec3(42u, 17u, 23u);
    uvec3 b = uvec3(0u, 0u, 0u);
    return a + b;
}

// run: test_uvec3_add_zero() == uvec3(42u, 17u, 23u)

uvec3 test_uvec3_add_variables() {
    uvec3 a = uvec3(15u, 10u, 5u);
    uvec3 b = uvec3(27u, 5u, 12u);
    return a + b;
}

// run: test_uvec3_add_variables() == uvec3(42u, 15u, 17u)

uvec3 test_uvec3_add_expressions() {
    return uvec3(8u, 4u, 6u) + uvec3(6u, 2u, 3u);
}

// run: test_uvec3_add_expressions() == uvec3(14u, 6u, 9u)

uvec3 test_uvec3_add_in_assignment() {
    uvec3 result = uvec3(5u, 3u, 2u);
    result = result + uvec3(10u, 7u, 8u);
    return result;
}

// run: test_uvec3_add_in_assignment() == uvec3(15u, 10u, 10u)

uvec3 test_uvec3_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    uvec3 a = uvec3(100000u, 50000u, 25000u);
    uvec3 b = uvec3(200000u, 30000u, 15000u);
    return a + b;
}

// run: test_uvec3_add_large_numbers() == uvec3(300000u, 80000u, 40000u)

uvec3 test_uvec3_add_max_values() {
    uvec3 a = uvec3(4294967295u, 4294967294u, 4294967293u);
    uvec3 b = uvec3(1u, 1u, 1u);
    return a + b;
}

// run: test_uvec3_add_max_values() == uvec3(0u, 4294967295u, 4294967294u)

uvec3 test_uvec3_add_mixed_components() {
    uvec3 a = uvec3(100u, 50u, 75u);
    uvec3 b = uvec3(200u, 75u, 150u);
    return a + b;
}

// run: test_uvec3_add_mixed_components() == uvec3(300u, 125u, 225u)

