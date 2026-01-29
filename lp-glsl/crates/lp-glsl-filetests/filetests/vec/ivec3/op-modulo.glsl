// test run
// target riscv32.q32

// ============================================================================
// Modulo: ivec3 % ivec3 -> ivec3 (component-wise, sign follows dividend)
// ============================================================================

ivec3 test_ivec3_modulo_positive_positive() {
    // Modulo operation (component-wise, sign follows dividend)
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(3, 4, 5);
    return a % b;
}

// run: test_ivec3_modulo_positive_positive() == ivec3(1, 3, 3)

ivec3 test_ivec3_modulo_positive_negative() {
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(-3, -4, -5);
    return a % b;
}

// run: test_ivec3_modulo_positive_negative() == ivec3(1, 3, 3)

ivec3 test_ivec3_modulo_negative_negative() {
    ivec3 a = ivec3(-10, -15, -8);
    ivec3 b = ivec3(-3, -4, -5);
    return a % b;
}

// run: test_ivec3_modulo_negative_negative() == ivec3(-1, -3, -3)

ivec3 test_ivec3_modulo_exact_division() {
    ivec3 a = ivec3(15, 20, 25);
    ivec3 b = ivec3(5, 4, 5);
    return a % b;
}

// run: test_ivec3_modulo_exact_division() == ivec3(0, 0, 0)

ivec3 test_ivec3_modulo_variables() {
    ivec3 a = ivec3(17, 19, 23);
    ivec3 b = ivec3(5, 7, 6);
    return a % b;
}

// run: test_ivec3_modulo_variables() == ivec3(2, 5, 5)

ivec3 test_ivec3_modulo_expressions() {
    return ivec3(20, 25, 30) % ivec3(7, 6, 8);
}

// run: test_ivec3_modulo_expressions() == ivec3(6, 1, 6)

ivec3 test_ivec3_modulo_in_assignment() {
    ivec3 result = ivec3(25, 30, 35);
    result = result % ivec3(7, 8, 9);
    return result;
}

// run: test_ivec3_modulo_in_assignment() == ivec3(4, 6, 8)

ivec3 test_ivec3_modulo_negative_dividend() {
    ivec3 a = ivec3(-17, -19, -23);
    ivec3 b = ivec3(5, 7, 6);
    return a % b;
}

// run: test_ivec3_modulo_negative_dividend() == ivec3(-2, -5, -5)

ivec3 test_ivec3_modulo_negative_divisor() {
    ivec3 a = ivec3(17, 19, 23);
    ivec3 b = ivec3(-5, -7, -6);
    return a % b;
}

// run: test_ivec3_modulo_negative_divisor() == ivec3(2, 5, 5)
