// test run
// target riscv32.q32

// ============================================================================
// Divide: uvec3 / uvec3 -> uvec3 (component-wise, truncates toward zero)
// ============================================================================

uvec3 test_uvec3_divide_positive_positive() {
    // Division with positive vectors (component-wise, truncates toward zero)
    uvec3 a = uvec3(10u, 15u, 20u);
    uvec3 b = uvec3(3u, 4u, 5u);
    return a / b;
}

// run: test_uvec3_divide_positive_positive() == uvec3(3u, 3u, 4u)

uvec3 test_uvec3_divide_by_one() {
    uvec3 a = uvec3(42u, 17u, 99u);
    uvec3 b = uvec3(1u, 1u, 1u);
    return a / b;
}

// run: test_uvec3_divide_by_one() == uvec3(42u, 17u, 99u)

uvec3 test_uvec3_divide_by_two() {
    uvec3 a = uvec3(20u, 30u, 40u);
    uvec3 b = uvec3(4u, 6u, 8u);
    return a / b;
}

// run: test_uvec3_divide_by_two() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_divide_variables() {
    uvec3 a = uvec3(24u, 18u, 36u);
    uvec3 b = uvec3(3u, 6u, 9u);
    return a / b;
}

// run: test_uvec3_divide_variables() == uvec3(8u, 3u, 4u)

uvec3 test_uvec3_divide_expressions() {
    return uvec3(50u, 40u, 60u) / uvec3(5u, 8u, 10u);
}

// run: test_uvec3_divide_expressions() == uvec3(10u, 5u, 6u)

uvec3 test_uvec3_divide_in_assignment() {
    uvec3 result = uvec3(15u, 20u, 25u);
    result = result / uvec3(3u, 4u, 5u);
    return result;
}

// run: test_uvec3_divide_in_assignment() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_divide_remainder() {
    uvec3 a = uvec3(17u, 19u, 23u);
    uvec3 b = uvec3(5u, 7u, 8u);
    return a / b;
}

// run: test_uvec3_divide_remainder() == uvec3(3u, 2u, 2u)

uvec3 test_uvec3_divide_mixed_components() {
    uvec3 a = uvec3(100u, 75u, 150u);
    uvec3 b = uvec3(10u, 25u, 30u);
    return a / b;
}

// run: test_uvec3_divide_mixed_components() == uvec3(10u, 3u, 5u)

uvec3 test_uvec3_divide_large_numbers() {
    uvec3 a = uvec3(100000u, 50000u, 25000u);
    uvec3 b = uvec3(1000u, 2500u, 5000u);
    return a / b;
}

// run: test_uvec3_divide_large_numbers() == uvec3(100u, 20u, 5u)
