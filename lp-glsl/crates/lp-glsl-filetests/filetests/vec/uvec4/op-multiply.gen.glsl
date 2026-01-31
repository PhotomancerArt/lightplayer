// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: uvec4 * uvec4 -> uvec4 (component-wise)
// ============================================================================

uvec4 test_uvec4_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    uvec4 a = uvec4(6u, 7u, 2u, 3u);
    uvec4 b = uvec4(2u, 3u, 4u, 5u);
    return a * b;
}

// run: test_uvec4_multiply_positive_positive() == uvec4(12u, 21u, 8u, 15u)

uvec4 test_uvec4_multiply_by_zero() {
    uvec4 a = uvec4(123u, 456u, 789u, 321u);
    uvec4 b = uvec4(0u, 0u, 0u, 0u);
    return a * b;
}

// run: test_uvec4_multiply_by_zero() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_multiply_by_one() {
    uvec4 a = uvec4(42u, 17u, 23u, 8u);
    uvec4 b = uvec4(1u, 1u, 1u, 1u);
    return a * b;
}

// run: test_uvec4_multiply_by_one() == uvec4(42u, 17u, 23u, 8u)

uvec4 test_uvec4_multiply_variables() {
    uvec4 a = uvec4(8u, 9u, 7u, 6u);
    uvec4 b = uvec4(7u, 6u, 5u, 4u);
    return a * b;
}

// run: test_uvec4_multiply_variables() == uvec4(56u, 54u, 35u, 24u)

uvec4 test_uvec4_multiply_expressions() {
    return uvec4(3u, 4u, 5u, 2u) * uvec4(5u, 2u, 1u, 6u);
}

// run: test_uvec4_multiply_expressions() == uvec4(15u, 8u, 5u, 12u)

uvec4 test_uvec4_multiply_in_assignment() {
    uvec4 result = uvec4(6u, 7u, 8u, 9u);
    result = result * uvec4(2u, 3u, 1u, 2u);
    return result;
}

// run: test_uvec4_multiply_in_assignment() == uvec4(12u, 21u, 8u, 18u)

uvec4 test_uvec4_multiply_large_numbers() {
    uvec4 a = uvec4(1000u, 2000u, 3000u, 4000u);
    uvec4 b = uvec4(3000u, 1000u, 2000u, 500u);
    return a * b;
}

// run: test_uvec4_multiply_large_numbers() == uvec4(3000000u, 2000000u, 6000000u, 2000000u)

uvec4 test_uvec4_multiply_overflow() {
    uvec4 a = uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u);
    uvec4 b = uvec4(2u, 2u, 2u, 2u);
    return a * b;
}

// run: test_uvec4_multiply_overflow() == uvec4(4294967294u, 4294967294u, 4294967294u, 4294967294u)

uvec4 test_uvec4_multiply_fractions() {
    uvec4 a = uvec4(3u, 4u, 5u, 2u);
    uvec4 b = uvec4(5u, 2u, 1u, 6u);
    return a * b;
}

// run: test_uvec4_multiply_fractions() == uvec4(15u, 8u, 5u, 12u)
