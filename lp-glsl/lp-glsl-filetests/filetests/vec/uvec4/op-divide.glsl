// test run
// target riscv32.q32

// ============================================================================
// Divide: uvec4 / uvec4 -> uvec4 (component-wise, truncates toward zero)
// ============================================================================

uvec4 test_uvec4_divide_positive_positive() {
    // Division with positive vectors (component-wise, truncates toward zero)
    uvec4 a = uvec4(10u, 15u, 20u, 25u);
    uvec4 b = uvec4(3u, 4u, 5u, 6u);
    return a / b;
}

// run: test_uvec4_divide_positive_positive() == uvec4(3u, 3u, 4u, 4u)

uvec4 test_uvec4_divide_by_one() {
    uvec4 a = uvec4(42u, 17u, 99u, 55u);
    uvec4 b = uvec4(1u, 1u, 1u, 1u);
    return a / b;
}

// run: test_uvec4_divide_by_one() == uvec4(42u, 17u, 99u, 55u)

uvec4 test_uvec4_divide_by_two() {
    uvec4 a = uvec4(20u, 30u, 40u, 50u);
    uvec4 b = uvec4(4u, 6u, 8u, 10u);
    return a / b;
}

// run: test_uvec4_divide_by_two() == uvec4(5u, 5u, 5u, 5u)

uvec4 test_uvec4_divide_variables() {
    uvec4 a = uvec4(24u, 18u, 36u, 48u);
    uvec4 b = uvec4(3u, 6u, 9u, 12u);
    return a / b;
}

// run: test_uvec4_divide_variables() == uvec4(8u, 3u, 4u, 4u)

uvec4 test_uvec4_divide_expressions() {
    return uvec4(50u, 40u, 60u, 70u) / uvec4(5u, 8u, 10u, 14u);
}

// run: test_uvec4_divide_expressions() == uvec4(10u, 5u, 6u, 5u)

uvec4 test_uvec4_divide_in_assignment() {
    uvec4 result = uvec4(15u, 20u, 25u, 30u);
    result = result / uvec4(3u, 4u, 5u, 6u);
    return result;
}

// run: test_uvec4_divide_in_assignment() == uvec4(5u, 5u, 5u, 5u)

uvec4 test_uvec4_divide_remainder() {
    uvec4 a = uvec4(17u, 19u, 23u, 31u);
    uvec4 b = uvec4(5u, 7u, 8u, 10u);
    return a / b;
}

// run: test_uvec4_divide_remainder() == uvec4(3u, 2u, 2u, 3u)

uvec4 test_uvec4_divide_mixed_components() {
    uvec4 a = uvec4(100u, 75u, 150u, 200u);
    uvec4 b = uvec4(10u, 25u, 30u, 40u);
    return a / b;
}

// run: test_uvec4_divide_mixed_components() == uvec4(10u, 3u, 5u, 5u)

uvec4 test_uvec4_divide_large_numbers() {
    uvec4 a = uvec4(100000u, 50000u, 25000u, 12500u);
    uvec4 b = uvec4(1000u, 2500u, 5000u, 2500u);
    return a / b;
}

// run: test_uvec4_divide_large_numbers() == uvec4(100u, 20u, 5u, 5u)
