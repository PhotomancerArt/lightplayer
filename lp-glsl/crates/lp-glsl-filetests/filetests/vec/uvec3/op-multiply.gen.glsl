// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: uvec3 * uvec3 -> uvec3 (component-wise)
// ============================================================================

uvec3 test_uvec3_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    uvec3 a = uvec3(6u, 7u, 2u);
    uvec3 b = uvec3(2u, 3u, 4u);
    return a * b;
}

// run: test_uvec3_multiply_positive_positive() == uvec3(12u, 21u, 8u)

uvec3 test_uvec3_multiply_by_zero() {
    uvec3 a = uvec3(123u, 456u, 789u);
    uvec3 b = uvec3(0u, 0u, 0u);
    return a * b;
}

// run: test_uvec3_multiply_by_zero() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_multiply_by_one() {
    uvec3 a = uvec3(42u, 17u, 23u);
    uvec3 b = uvec3(1u, 1u, 1u);
    return a * b;
}

// run: test_uvec3_multiply_by_one() == uvec3(42u, 17u, 23u)

uvec3 test_uvec3_multiply_variables() {
    uvec3 a = uvec3(8u, 9u, 7u);
    uvec3 b = uvec3(7u, 6u, 5u);
    return a * b;
}

// run: test_uvec3_multiply_variables() == uvec3(56u, 54u, 35u)

uvec3 test_uvec3_multiply_expressions() {
    return uvec3(3u, 4u, 5u) * uvec3(5u, 2u, 1u);
}

// run: test_uvec3_multiply_expressions() == uvec3(15u, 8u, 5u)

uvec3 test_uvec3_multiply_in_assignment() {
    uvec3 result = uvec3(6u, 7u, 8u);
    result = result * uvec3(2u, 3u, 1u);
    return result;
}

// run: test_uvec3_multiply_in_assignment() == uvec3(12u, 21u, 8u)

uvec3 test_uvec3_multiply_large_numbers() {
    uvec3 a = uvec3(1000u, 2000u, 3000u);
    uvec3 b = uvec3(3000u, 1000u, 2000u);
    return a * b;
}

// run: test_uvec3_multiply_large_numbers() == uvec3(3000000u, 2000000u, 6000000u)

uvec3 test_uvec3_multiply_overflow() {
    uvec3 a = uvec3(4294967295u, 4294967295u, 4294967295u);
    uvec3 b = uvec3(2u, 2u, 2u);
    return a * b;
}

// run: test_uvec3_multiply_overflow() == uvec3(4294967294u, 4294967294u, 4294967294u)

uvec3 test_uvec3_multiply_fractions() {
    uvec3 a = uvec3(3u, 4u, 5u);
    uvec3 b = uvec3(5u, 2u, 1u);
    return a * b;
}

// run: test_uvec3_multiply_fractions() == uvec3(15u, 8u, 5u)
