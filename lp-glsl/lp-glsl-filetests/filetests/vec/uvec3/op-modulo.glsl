// test run
// target riscv32.q32

// ============================================================================
// Modulo: uvec3 % uvec3 -> uvec3 (component-wise)
// ============================================================================

uvec3 test_uvec3_modulo_positive_positive() {
    // Modulo operation (component-wise)
    uvec3 a = uvec3(10u, 15u, 20u);
    uvec3 b = uvec3(3u, 4u, 5u);
    return a % b;
}

// run: test_uvec3_modulo_positive_positive() == uvec3(1u, 3u, 0u)

uvec3 test_uvec3_modulo_exact_division() {
    uvec3 a = uvec3(15u, 20u, 25u);
    uvec3 b = uvec3(5u, 4u, 5u);
    return a % b;
}

// run: test_uvec3_modulo_exact_division() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_modulo_variables() {
    uvec3 a = uvec3(17u, 19u, 23u);
    uvec3 b = uvec3(5u, 7u, 8u);
    return a % b;
}

// run: test_uvec3_modulo_variables() == uvec3(2u, 5u, 7u)

uvec3 test_uvec3_modulo_expressions() {
    return uvec3(20u, 25u, 30u) % uvec3(7u, 6u, 11u);
}

// run: test_uvec3_modulo_expressions() == uvec3(6u, 1u, 8u)

uvec3 test_uvec3_modulo_in_assignment() {
    uvec3 result = uvec3(25u, 30u, 35u);
    result = result % uvec3(7u, 8u, 9u);
    return result;
}

// run: test_uvec3_modulo_in_assignment() == uvec3(4u, 6u, 8u)

uvec3 test_uvec3_modulo_by_one() {
    uvec3 a = uvec3(42u, 17u, 99u);
    uvec3 b = uvec3(1u, 1u, 1u);
    return a % b;
}

// run: test_uvec3_modulo_by_one() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_modulo_same_values() {
    uvec3 a = uvec3(10u, 15u, 20u);
    uvec3 b = uvec3(10u, 15u, 20u);
    return a % b;
}

// run: test_uvec3_modulo_same_values() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_modulo_large_numbers() {
    uvec3 a = uvec3(100000u, 50000u, 25000u);
    uvec3 b = uvec3(3000u, 7000u, 8000u);
    return a % b;
}

// run: test_uvec3_modulo_large_numbers() == uvec3(1000u, 1000u, 1000u)

uvec3 test_uvec3_modulo_mixed_components() {
    uvec3 a = uvec3(123u, 456u, 789u);
    uvec3 b = uvec3(50u, 200u, 300u);
    return a % b;
}

// run: test_uvec3_modulo_mixed_components() == uvec3(23u, 56u, 189u)
