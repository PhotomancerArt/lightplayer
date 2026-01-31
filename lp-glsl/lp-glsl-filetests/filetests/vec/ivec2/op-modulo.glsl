// test run
// target riscv32.q32

// ============================================================================
// Modulo: ivec2 % ivec2 -> ivec2 (component-wise, sign follows dividend)
// ============================================================================

ivec2 test_ivec2_modulo_positive_positive() {
    // Modulo operation (component-wise, sign follows dividend)
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(3, 4);
    return a % b;
}

// run: test_ivec2_modulo_positive_positive() == ivec2(1, 3)

ivec2 test_ivec2_modulo_positive_negative() {
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(-3, -4);
    return a % b;
}

// run: test_ivec2_modulo_positive_negative() == ivec2(1, 3)

ivec2 test_ivec2_modulo_negative_negative() {
    ivec2 a = ivec2(-10, -15);
    ivec2 b = ivec2(-3, -4);
    return a % b;
}

// run: test_ivec2_modulo_negative_negative() == ivec2(-1, -3)

ivec2 test_ivec2_modulo_exact_division() {
    ivec2 a = ivec2(15, 20);
    ivec2 b = ivec2(5, 4);
    return a % b;
}

// run: test_ivec2_modulo_exact_division() == ivec2(0, 0)

ivec2 test_ivec2_modulo_variables() {
    ivec2 a = ivec2(17, 19);
    ivec2 b = ivec2(5, 7);
    return a % b;
}

// run: test_ivec2_modulo_variables() == ivec2(2, 5)

ivec2 test_ivec2_modulo_expressions() {
    return ivec2(20, 25) % ivec2(7, 6);
}

// run: test_ivec2_modulo_expressions() == ivec2(6, 1)

ivec2 test_ivec2_modulo_in_assignment() {
    ivec2 result = ivec2(25, 30);
    result = result % ivec2(7, 8);
    return result;
}

// run: test_ivec2_modulo_in_assignment() == ivec2(4, 6)

ivec2 test_ivec2_modulo_negative_dividend() {
    ivec2 a = ivec2(-17, -19);
    ivec2 b = ivec2(5, 7);
    return a % b;
}

// run: test_ivec2_modulo_negative_dividend() == ivec2(-2, -5)

ivec2 test_ivec2_modulo_negative_divisor() {
    ivec2 a = ivec2(17, 19);
    ivec2 b = ivec2(-5, -7);
    return a % b;
}

// run: test_ivec2_modulo_negative_divisor() == ivec2(2, 5)
