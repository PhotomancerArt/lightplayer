// test run
// target riscv32.q32

// ============================================================================
// Equal: mat3 == mat3 -> bool (aggregate equality)
// ============================================================================

bool test_mat3_equal_true() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return a == b;
}

// run: test_mat3_equal_true() == true

bool test_mat3_equal_false() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0);
    return a == b;
}

// run: test_mat3_equal_false() == false

bool test_mat3_equal_identity() {
    mat3 a = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    mat3 b = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return a == b;
}

// run: test_mat3_equal_identity() == true

bool test_mat3_equal_zero() {
    mat3 a = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    mat3 b = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return a == b;
}

// run: test_mat3_equal_zero() == true

bool test_mat3_equal_variables() {
    mat3 a = mat3(2.5, 3.7, 4.2, 5.1, 6.3, 7.4, 8.5, 9.6, 10.7);
    mat3 b = mat3(2.5, 3.7, 4.2, 5.1, 6.3, 7.4, 8.5, 9.6, 10.7);
    return a == b;
}

// run: test_mat3_equal_variables() == true

bool test_mat3_equal_expressions() {
    return mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) == mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
}

// run: test_mat3_equal_expressions() == true

bool test_mat3_equal_different_order() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
    return a == b;
}

// run: test_mat3_equal_different_order() == false

bool test_mat3_equal_negative() {
    mat3 a = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    mat3 b = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return a == b;
}

// run: test_mat3_equal_negative() == true

bool test_mat3_equal_after_assignment() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    b = a;
    return a == b;
}

// run: test_mat3_equal_after_assignment() == true




