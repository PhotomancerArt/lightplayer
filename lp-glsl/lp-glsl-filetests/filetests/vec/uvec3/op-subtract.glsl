// test run
// target riscv32.q32

// ============================================================================
// Subtract: uvec3 - uvec3 -> uvec3 (component-wise)
// ============================================================================

uvec3 test_uvec3_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    uvec3 a = uvec3(10u, 8u, 6u);
    uvec3 b = uvec3(3u, 2u, 1u);
    return a - b;
}

// run: test_uvec3_subtract_positive_positive() == uvec3(7u, 6u, 5u)

uvec3 test_uvec3_subtract_positive_zero() {
    uvec3 a = uvec3(42u, 17u, 99u);
    uvec3 b = uvec3(0u, 0u, 0u);
    return a - b;
}

// run: test_uvec3_subtract_positive_zero() == uvec3(42u, 17u, 99u)

uvec3 test_uvec3_subtract_variables() {
    uvec3 a = uvec3(50u, 20u, 30u);
    uvec3 b = uvec3(15u, 5u, 10u);
    return a - b;
}

// run: test_uvec3_subtract_variables() == uvec3(35u, 15u, 20u)

uvec3 test_uvec3_subtract_expressions() {
    return uvec3(20u, 10u, 15u) - uvec3(5u, 3u, 7u);
}

// run: test_uvec3_subtract_expressions() == uvec3(15u, 7u, 8u)

uvec3 test_uvec3_subtract_in_assignment() {
    uvec3 result = uvec3(20u, 15u, 25u);
    result = result - uvec3(8u, 5u, 10u);
    return result;
}

// run: test_uvec3_subtract_in_assignment() == uvec3(12u, 10u, 15u)

uvec3 test_uvec3_subtract_large_numbers() {
    uvec3 a = uvec3(500000u, 300000u, 200000u);
    uvec3 b = uvec3(200000u, 100000u, 50000u);
    return a - b;
}

// run: test_uvec3_subtract_large_numbers() == uvec3(300000u, 200000u, 150000u)

uvec3 test_uvec3_subtract_underflow() {
    uvec3 a = uvec3(1u, 0u, 2u);
    uvec3 b = uvec3(2u, 1u, 3u);
    return a - b;
}

// run: test_uvec3_subtract_underflow() == uvec3(4294967295u, 4294967295u, 4294967295u)

uvec3 test_uvec3_subtract_mixed_components() {
    uvec3 a = uvec3(100u, 80u, 60u);
    uvec3 b = uvec3(30u, 40u, 20u);
    return a - b;
}

// run: test_uvec3_subtract_mixed_components() == uvec3(70u, 40u, 40u)
