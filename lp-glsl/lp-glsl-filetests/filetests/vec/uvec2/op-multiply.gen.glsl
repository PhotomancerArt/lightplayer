// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: uvec2 * uvec2 -> uvec2 (component-wise)
// ============================================================================

uvec2 test_uvec2_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    uvec2 a = uvec2(6u, 7u);
    uvec2 b = uvec2(2u, 3u);
    return a * b;
}

// run: test_uvec2_multiply_positive_positive() == uvec2(12u, 21u)

uvec2 test_uvec2_multiply_by_zero() {
    uvec2 a = uvec2(123u, 456u);
    uvec2 b = uvec2(0u, 0u);
    return a * b;
}

// run: test_uvec2_multiply_by_zero() == uvec2(0u, 0u)

uvec2 test_uvec2_multiply_by_one() {
    uvec2 a = uvec2(42u, 17u);
    uvec2 b = uvec2(1u, 1u);
    return a * b;
}

// run: test_uvec2_multiply_by_one() == uvec2(42u, 17u)

uvec2 test_uvec2_multiply_variables() {
    uvec2 a = uvec2(8u, 9u);
    uvec2 b = uvec2(7u, 6u);
    return a * b;
}

// run: test_uvec2_multiply_variables() == uvec2(56u, 54u)

uvec2 test_uvec2_multiply_expressions() {
    return uvec2(3u, 4u) * uvec2(5u, 2u);
}

// run: test_uvec2_multiply_expressions() == uvec2(15u, 8u)

uvec2 test_uvec2_multiply_in_assignment() {
    uvec2 result = uvec2(6u, 7u);
    result = result * uvec2(2u, 3u);
    return result;
}

// run: test_uvec2_multiply_in_assignment() == uvec2(12u, 21u)

uvec2 test_uvec2_multiply_large_numbers() {
    uvec2 a = uvec2(1000u, 2000u);
    uvec2 b = uvec2(3000u, 1000u);
    return a * b;
}

// run: test_uvec2_multiply_large_numbers() == uvec2(3000000u, 2000000u)

uvec2 test_uvec2_multiply_overflow() {
    uvec2 a = uvec2(4294967295u, 4294967295u);
    uvec2 b = uvec2(2u, 2u);
    return a * b;
}

// run: test_uvec2_multiply_overflow() == uvec2(4294967294u, 4294967294u)

uvec2 test_uvec2_multiply_fractions() {
    uvec2 a = uvec2(3u, 4u);
    uvec2 b = uvec2(5u, 2u);
    return a * b;
}

// run: test_uvec2_multiply_fractions() == uvec2(15u, 8u)
