// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: uvec2 + uvec2 -> uvec2 (component-wise)
// ============================================================================

uvec2 test_uvec2_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    return a + b;
}

// run: test_uvec2_add_positive_positive() == uvec2(7u, 7u)

uvec2 test_uvec2_add_zero() {
    uvec2 a = uvec2(42u, 17u);
    uvec2 b = uvec2(0u, 0u);
    return a + b;
}

// run: test_uvec2_add_zero() == uvec2(42u, 17u)

uvec2 test_uvec2_add_variables() {
    uvec2 a = uvec2(15u, 10u);
    uvec2 b = uvec2(27u, 5u);
    return a + b;
}

// run: test_uvec2_add_variables() == uvec2(42u, 15u)

uvec2 test_uvec2_add_expressions() {
    return uvec2(8u, 4u) + uvec2(6u, 2u);
}

// run: test_uvec2_add_expressions() == uvec2(14u, 6u)

uvec2 test_uvec2_add_in_assignment() {
    uvec2 result = uvec2(5u, 3u);
    result = result + uvec2(10u, 7u);
    return result;
}

// run: test_uvec2_add_in_assignment() == uvec2(15u, 10u)

uvec2 test_uvec2_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    uvec2 a = uvec2(100000u, 50000u);
    uvec2 b = uvec2(200000u, 30000u);
    return a + b;
}

// run: test_uvec2_add_large_numbers() == uvec2(300000u, 80000u)

uvec2 test_uvec2_add_max_values() {
    uvec2 a = uvec2(4294967295u, 4294967294u);
    uvec2 b = uvec2(1u, 1u);
    return a + b;
}

// run: test_uvec2_add_max_values() == uvec2(0u, 4294967295u)

uvec2 test_uvec2_add_mixed_components() {
    uvec2 a = uvec2(100u, 50u);
    uvec2 b = uvec2(200u, 75u);
    return a + b;
}

// run: test_uvec2_add_mixed_components() == uvec2(300u, 125u)

