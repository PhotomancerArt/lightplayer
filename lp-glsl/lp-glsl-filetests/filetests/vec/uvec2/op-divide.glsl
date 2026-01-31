// test run
// target riscv32.q32

// ============================================================================
// Divide: uvec2 / uvec2 -> uvec2 (component-wise, truncates toward zero)
// ============================================================================

uvec2 test_uvec2_divide_positive_positive() {
    // Division with positive vectors (component-wise, truncates toward zero)
    uvec2 a = uvec2(10u, 15u);
    uvec2 b = uvec2(3u, 4u);
    return a / b;
}

// run: test_uvec2_divide_positive_positive() == uvec2(3u, 3u)

uvec2 test_uvec2_divide_by_one() {
    uvec2 a = uvec2(42u, 17u);
    uvec2 b = uvec2(1u, 1u);
    return a / b;
}

// run: test_uvec2_divide_by_one() == uvec2(42u, 17u)

uvec2 test_uvec2_divide_by_two() {
    uvec2 a = uvec2(20u, 30u);
    uvec2 b = uvec2(4u, 6u);
    return a / b;
}

// run: test_uvec2_divide_by_two() == uvec2(5u, 5u)

uvec2 test_uvec2_divide_variables() {
    uvec2 a = uvec2(24u, 18u);
    uvec2 b = uvec2(3u, 6u);
    return a / b;
}

// run: test_uvec2_divide_variables() == uvec2(8u, 3u)

uvec2 test_uvec2_divide_expressions() {
    return uvec2(50u, 40u) / uvec2(5u, 8u);
}

// run: test_uvec2_divide_expressions() == uvec2(10u, 5u)

uvec2 test_uvec2_divide_in_assignment() {
    uvec2 result = uvec2(15u, 20u);
    result = result / uvec2(3u, 4u);
    return result;
}

// run: test_uvec2_divide_in_assignment() == uvec2(5u, 5u)

uvec2 test_uvec2_divide_remainder() {
    uvec2 a = uvec2(17u, 19u);
    uvec2 b = uvec2(5u, 7u);
    return a / b;
}

// run: test_uvec2_divide_remainder() == uvec2(3u, 2u)

uvec2 test_uvec2_divide_mixed_components() {
    uvec2 a = uvec2(100u, 75u);
    uvec2 b = uvec2(10u, 25u);
    return a / b;
}

// run: test_uvec2_divide_mixed_components() == uvec2(10u, 3u)

uvec2 test_uvec2_divide_large_numbers() {
    uvec2 a = uvec2(100000u, 50000u);
    uvec2 b = uvec2(1000u, 2500u);
    return a / b;
}

// run: test_uvec2_divide_large_numbers() == uvec2(100u, 20u)
