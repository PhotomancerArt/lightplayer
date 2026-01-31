// test run
// target riscv32.q32

// ============================================================================
// Multiply Assign: mat2 *= mat2 (matrix multiplication) or mat2 *= float (component-wise)
// ============================================================================

mat2 test_mat2_multiply_assign_matrix() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= mat2(1.0, 0.0, 0.0, 1.0); // multiply by identity
    return result;
}

// run: test_mat2_multiply_assign_matrix() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_multiply_assign_matrix_simple() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= mat2(2.0, 0.0, 0.0, 2.0); // scale by 2
    return result;
}

// run: test_mat2_multiply_assign_matrix_simple() ~= mat2(2.0, 4.0, 6.0, 8.0)

mat2 test_mat2_multiply_assign_matrix_variables() {
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0); // identity
    mat2 b = mat2(2.0, 3.0, 4.0, 5.0);
    a *= b;
    return a;
}

// run: test_mat2_multiply_assign_matrix_variables() ~= mat2(2.0, 3.0, 4.0, 5.0)

mat2 test_mat2_multiply_assign_matrix_expressions() {
    mat2 result = mat2(1.0, 1.0, 0.0, 1.0);
    result *= mat2(1.0, 0.0, 1.0, 1.0) * mat2(1.0, 1.0, 0.0, 1.0);
    return result;
}

// run: test_mat2_multiply_assign_matrix_expressions() ~= mat2(2.0, 2.0, 1.0, 2.0) [expect-fail]

mat2 test_mat2_multiply_assign_scalar() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= 2.0;
    return result;
}

// run: test_mat2_multiply_assign_scalar() ~= mat2(2.0, 4.0, 6.0, 8.0)

mat2 test_mat2_multiply_assign_scalar_zero() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= 0.0;
    return result;
}

// run: test_mat2_multiply_assign_scalar_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_multiply_assign_scalar_variables() {
    mat2 a = mat2(2.0, 4.0, 6.0, 8.0);
    float s = 0.5;
    a *= s;
    return a;
}

// run: test_mat2_multiply_assign_scalar_variables() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_multiply_assign_scalar_expressions() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= (2.0 + 1.0) / 3.0; // multiply by 1.0
    return result;
}

// run: test_mat2_multiply_assign_scalar_expressions() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_multiply_assign_scalar_negative() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result *= -1.0;
    return result;
}

// run: test_mat2_multiply_assign_scalar_negative() ~= mat2(-1.0, -2.0, -3.0, -4.0)

mat2 test_mat2_multiply_assign_scalar_large() {
    mat2 result = mat2(1.0, 1.0, 1.0, 1.0);
    result *= 1000.0;
    return result;
}

// run: test_mat2_multiply_assign_scalar_large() ~= mat2(1000.0, 1000.0, 1000.0, 1000.0)




