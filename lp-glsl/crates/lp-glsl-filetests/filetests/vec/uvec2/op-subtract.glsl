// test run
// target riscv32.q32

// ============================================================================
// Subtract: uvec2 - uvec2 -> uvec2 (component-wise)
// ============================================================================

uvec2 test_uvec2_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    uvec2 a = uvec2(10u, 8u);
    uvec2 b = uvec2(3u, 2u);
    return a - b;
}

// run: test_uvec2_subtract_positive_positive() == uvec2(7u, 6u)

uvec2 test_uvec2_subtract_positive_zero() {
    uvec2 a = uvec2(42u, 17u);
    uvec2 b = uvec2(0u, 0u);
    return a - b;
}

// run: test_uvec2_subtract_positive_zero() == uvec2(42u, 17u)

uvec2 test_uvec2_subtract_variables() {
    uvec2 a = uvec2(50u, 20u);
    uvec2 b = uvec2(15u, 5u);
    return a - b;
}

// run: test_uvec2_subtract_variables() == uvec2(35u, 15u)

uvec2 test_uvec2_subtract_expressions() {
    return uvec2(20u, 10u) - uvec2(5u, 3u);
}

// run: test_uvec2_subtract_expressions() == uvec2(15u, 7u)

uvec2 test_uvec2_subtract_in_assignment() {
    uvec2 result = uvec2(20u, 15u);
    result = result - uvec2(8u, 5u);
    return result;
}

// run: test_uvec2_subtract_in_assignment() == uvec2(12u, 10u)

uvec2 test_uvec2_subtract_large_numbers() {
    uvec2 a = uvec2(500000u, 300000u);
    uvec2 b = uvec2(200000u, 100000u);
    return a - b;
}

// run: test_uvec2_subtract_large_numbers() == uvec2(300000u, 200000u)

uvec2 test_uvec2_subtract_underflow() {
    uvec2 a = uvec2(1u, 0u);
    uvec2 b = uvec2(2u, 1u);
    return a - b;
}

// run: test_uvec2_subtract_underflow() == uvec2(4294967295u, 4294967295u)

uvec2 test_uvec2_subtract_mixed_components() {
    uvec2 a = uvec2(100u, 80u);
    uvec2 b = uvec2(30u, 40u);
    return a - b;
}

// run: test_uvec2_subtract_mixed_components() == uvec2(70u, 40u)
