// test run
// target riscv32.q32

// ============================================================================
// Modulo: uvec2 % uvec2 -> uvec2 (component-wise)
// ============================================================================

uvec2 test_uvec2_modulo_positive_positive() {
    // Modulo operation (component-wise)
    uvec2 a = uvec2(10u, 15u);
    uvec2 b = uvec2(3u, 4u);
    return a % b;
}

// run: test_uvec2_modulo_positive_positive() == uvec2(1u, 3u)

uvec2 test_uvec2_modulo_exact_division() {
    uvec2 a = uvec2(15u, 20u);
    uvec2 b = uvec2(5u, 4u);
    return a % b;
}

// run: test_uvec2_modulo_exact_division() == uvec2(0u, 0u)

uvec2 test_uvec2_modulo_variables() {
    uvec2 a = uvec2(17u, 19u);
    uvec2 b = uvec2(5u, 7u);
    return a % b;
}

// run: test_uvec2_modulo_variables() == uvec2(2u, 5u)

uvec2 test_uvec2_modulo_expressions() {
    return uvec2(20u, 25u) % uvec2(7u, 6u);
}

// run: test_uvec2_modulo_expressions() == uvec2(6u, 1u)

uvec2 test_uvec2_modulo_in_assignment() {
    uvec2 result = uvec2(25u, 30u);
    result = result % uvec2(7u, 8u);
    return result;
}

// run: test_uvec2_modulo_in_assignment() == uvec2(4u, 6u)

uvec2 test_uvec2_modulo_by_one() {
    uvec2 a = uvec2(42u, 17u);
    uvec2 b = uvec2(1u, 1u);
    return a % b;
}

// run: test_uvec2_modulo_by_one() == uvec2(0u, 0u)

uvec2 test_uvec2_modulo_same_values() {
    uvec2 a = uvec2(10u, 15u);
    uvec2 b = uvec2(10u, 15u);
    return a % b;
}

// run: test_uvec2_modulo_same_values() == uvec2(0u, 0u)

uvec2 test_uvec2_modulo_large_numbers() {
    uvec2 a = uvec2(100000u, 50000u);
    uvec2 b = uvec2(3000u, 7000u);
    return a % b;
}

// run: test_uvec2_modulo_large_numbers() == uvec2(1000u, 1000u)

uvec2 test_uvec2_modulo_mixed_components() {
    uvec2 a = uvec2(123u, 456u);
    uvec2 b = uvec2(50u, 200u);
    return a % b;
}

// run: test_uvec2_modulo_mixed_components() == uvec2(23u, 56u)
