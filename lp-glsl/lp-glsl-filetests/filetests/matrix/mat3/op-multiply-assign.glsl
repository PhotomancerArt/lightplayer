// test run
// target riscv32.q32

// ============================================================================
// Multiply Assign: mat3 *= mat3 (matrix multiplication) or mat3 *= float (component-wise)
// ============================================================================

mat3 test_mat3_multiply_assign_matrix() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // multiply by identity
    return result;
}

// run: test_mat3_multiply_assign_matrix() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_multiply_assign_matrix_simple() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= mat3(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0); // scale by 2
    return result;
}

// run: test_mat3_multiply_assign_matrix_simple() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0)

mat3 test_mat3_multiply_assign_matrix_variables() {
    mat3 a = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // identity
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    a *= b;
    return a;
}

// run: test_mat3_multiply_assign_matrix_variables() ~= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0)

mat3 test_mat3_multiply_assign_matrix_expressions() {
    mat3 result = mat3(1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    result *= mat3(1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0) * mat3(1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return result;
}

// run: test_mat3_multiply_assign_matrix_expressions() ~= mat3(2.0, 2.0, 2.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0) [expect-fail]

mat3 test_mat3_multiply_assign_scalar() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= 2.0;
    return result;
}

// run: test_mat3_multiply_assign_scalar() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0)

mat3 test_mat3_multiply_assign_scalar_zero() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= 0.0;
    return result;
}

// run: test_mat3_multiply_assign_scalar_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_multiply_assign_scalar_variables() {
    mat3 a = mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0);
    float s = 0.5;
    a *= s;
    return a;
}

// run: test_mat3_multiply_assign_scalar_variables() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_multiply_assign_scalar_expressions() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= (2.0 + 1.0) / 3.0; // multiply by 1.0
    return result;
}

// run: test_mat3_multiply_assign_scalar_expressions() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_multiply_assign_scalar_negative() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result *= -1.0;
    return result;
}

// run: test_mat3_multiply_assign_scalar_negative() ~= mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0)

mat3 test_mat3_multiply_assign_scalar_large() {
    mat3 result = mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    result *= 1000.0;
    return result;
}

// run: test_mat3_multiply_assign_scalar_large() ~= mat3(1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 1000.0)




