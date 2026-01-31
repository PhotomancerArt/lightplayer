// test run
// target riscv32.q32

// ============================================================================
// Modulo: uvec4 % uvec4 -> uvec4 (component-wise)
// ============================================================================

uvec4 test_uvec4_modulo_positive_positive() {
    // Modulo operation (component-wise)
    uvec4 a = uvec4(10u, 15u, 20u, 25u);
    uvec4 b = uvec4(3u, 4u, 5u, 6u);
    return a % b;
}

// run: test_uvec4_modulo_positive_positive() == uvec4(1u, 3u, 0u, 1u)

uvec4 test_uvec4_modulo_exact_division() {
    uvec4 a = uvec4(15u, 20u, 25u, 30u);
    uvec4 b = uvec4(5u, 4u, 5u, 6u);
    return a % b;
}

// run: test_uvec4_modulo_exact_division() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_modulo_variables() {
    uvec4 a = uvec4(17u, 19u, 23u, 31u);
    uvec4 b = uvec4(5u, 7u, 8u, 10u);
    return a % b;
}

// run: test_uvec4_modulo_variables() == uvec4(2u, 5u, 7u, 1u)

uvec4 test_uvec4_modulo_expressions() {
    return uvec4(20u, 25u, 30u, 35u) % uvec4(7u, 6u, 11u, 12u);
}

// run: test_uvec4_modulo_expressions() == uvec4(6u, 1u, 8u, 11u)

uvec4 test_uvec4_modulo_in_assignment() {
    uvec4 result = uvec4(25u, 30u, 35u, 40u);
    result = result % uvec4(7u, 8u, 9u, 11u);
    return result;
}

// run: test_uvec4_modulo_in_assignment() == uvec4(4u, 6u, 8u, 7u)

uvec4 test_uvec4_modulo_by_one() {
    uvec4 a = uvec4(42u, 17u, 99u, 55u);
    uvec4 b = uvec4(1u, 1u, 1u, 1u);
    return a % b;
}

// run: test_uvec4_modulo_by_one() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_modulo_same_values() {
    uvec4 a = uvec4(10u, 15u, 20u, 25u);
    uvec4 b = uvec4(10u, 15u, 20u, 25u);
    return a % b;
}

// run: test_uvec4_modulo_same_values() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_modulo_large_numbers() {
    uvec4 a = uvec4(100000u, 50000u, 25000u, 12500u);
    uvec4 b = uvec4(3000u, 7000u, 8000u, 9000u);
    return a % b;
}

// run: test_uvec4_modulo_large_numbers() == uvec4(1000u, 1000u, 1000u, 3500u)

uvec4 test_uvec4_modulo_mixed_components() {
    uvec4 a = uvec4(123u, 456u, 789u, 1000u);
    uvec4 b = uvec4(50u, 200u, 300u, 400u);
    return a % b;
}

// run: test_uvec4_modulo_mixed_components() == uvec4(23u, 56u, 189u, 200u)
