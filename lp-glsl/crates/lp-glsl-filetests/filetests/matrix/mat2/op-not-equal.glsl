// test run
// target riscv32.q32

// ============================================================================
// Not Equal: mat2 != mat2 -> bool (aggregate inequality)
// ============================================================================

bool test_mat2_not_equal_false() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(1.0, 2.0, 3.0, 4.0);
    return a != b;
}

// run: test_mat2_not_equal_false() == false

bool test_mat2_not_equal_true() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(1.0, 2.0, 3.0, 5.0);
    return a != b;
}

// run: test_mat2_not_equal_true() == true

bool test_mat2_not_equal_identity() {
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0);
    mat2 b = mat2(1.0, 0.0, 0.0, 1.0);
    return a != b;
}

// run: test_mat2_not_equal_identity() == false

bool test_mat2_not_equal_zero() {
    mat2 a = mat2(0.0, 0.0, 0.0, 0.0);
    mat2 b = mat2(0.0, 0.0, 0.0, 1.0);
    return a != b;
}

// run: test_mat2_not_equal_zero() == true

bool test_mat2_not_equal_variables() {
    mat2 a = mat2(2.5, 3.7, 4.2, 5.1);
    mat2 b = mat2(2.5, 3.7, 4.2, 5.2);
    return a != b;
}

// run: test_mat2_not_equal_variables() == true

bool test_mat2_not_equal_expressions() {
    return mat2(1.0, 2.0, 3.0, 4.0) != mat2(1.0, 2.0, 3.0, 4.0);
}

// run: test_mat2_not_equal_expressions() == false

bool test_mat2_not_equal_different_order() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(4.0, 3.0, 2.0, 1.0);
    return a != b;
}

// run: test_mat2_not_equal_different_order() == true

bool test_mat2_not_equal_negative() {
    mat2 a = mat2(-1.0, -2.0, -3.0, -4.0);
    mat2 b = mat2(-1.0, -2.0, -3.0, -5.0);
    return a != b;
}

// run: test_mat2_not_equal_negative() == true

bool test_mat2_not_equal_after_assignment() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(5.0, 6.0, 7.0, 8.0);
    b = a;
    return a != b;
}

// run: test_mat2_not_equal_after_assignment() == false




