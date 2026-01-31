// test run
// target riscv32.q32

// ============================================================================
// Subtract: uvec4 - uvec4 -> uvec4 (component-wise)
// ============================================================================

uvec4 test_uvec4_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    uvec4 a = uvec4(10u, 8u, 6u, 4u);
    uvec4 b = uvec4(3u, 2u, 1u, 2u);
    return a - b;
}

// run: test_uvec4_subtract_positive_positive() == uvec4(7u, 6u, 5u, 2u)

uvec4 test_uvec4_subtract_positive_zero() {
    uvec4 a = uvec4(42u, 17u, 99u, 55u);
    uvec4 b = uvec4(0u, 0u, 0u, 0u);
    return a - b;
}

// run: test_uvec4_subtract_positive_zero() == uvec4(42u, 17u, 99u, 55u)

uvec4 test_uvec4_subtract_variables() {
    uvec4 a = uvec4(50u, 20u, 30u, 40u);
    uvec4 b = uvec4(15u, 5u, 10u, 20u);
    return a - b;
}

// run: test_uvec4_subtract_variables() == uvec4(35u, 15u, 20u, 20u)

uvec4 test_uvec4_subtract_expressions() {
    return uvec4(20u, 10u, 15u, 25u) - uvec4(5u, 3u, 7u, 10u);
}

// run: test_uvec4_subtract_expressions() == uvec4(15u, 7u, 8u, 15u)

uvec4 test_uvec4_subtract_in_assignment() {
    uvec4 result = uvec4(20u, 15u, 25u, 30u);
    result = result - uvec4(8u, 5u, 10u, 12u);
    return result;
}

// run: test_uvec4_subtract_in_assignment() == uvec4(12u, 10u, 15u, 18u)

uvec4 test_uvec4_subtract_large_numbers() {
    uvec4 a = uvec4(500000u, 300000u, 200000u, 100000u);
    uvec4 b = uvec4(200000u, 100000u, 50000u, 25000u);
    return a - b;
}

// run: test_uvec4_subtract_large_numbers() == uvec4(300000u, 200000u, 150000u, 75000u)

uvec4 test_uvec4_subtract_underflow() {
    uvec4 a = uvec4(1u, 0u, 2u, 3u);
    uvec4 b = uvec4(2u, 1u, 3u, 4u);
    return a - b;
}

// run: test_uvec4_subtract_underflow() == uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u)

uvec4 test_uvec4_subtract_mixed_components() {
    uvec4 a = uvec4(100u, 80u, 60u, 40u);
    uvec4 b = uvec4(30u, 40u, 20u, 10u);
    return a - b;
}

// run: test_uvec4_subtract_mixed_components() == uvec4(70u, 40u, 40u, 30u)
