// test run
// target riscv32.q32

// ============================================================================
// Multiply: mat3 * mat3 -> mat3 (matrix multiplication)
// ============================================================================

mat3 test_mat3_multiply_identity() {
    // Matrix multiplication with identity matrix
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 identity = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return a * identity;
}

// run: test_mat3_multiply_identity() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_multiply_scale() {
    // Scaling matrix multiplication
    mat3 a = mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0); // scale matrix
    mat3 b = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return a * b;
}

// run: test_mat3_multiply_scale() ~= mat3(2.0, 4.0, 6.0, 12.0, 15.0, 18.0, 28.0, 32.0, 36.0) [expect-fail]

mat3 test_mat3_multiply_simple() {
    // Simple 3x3 matrix multiplication
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
    // This is a complex calculation - each element is sum of row*column
    return a * b;
    // Result depends on actual matrix multiplication calculation
}

// run: test_mat3_multiply_simple() ~= mat3(30.0, 24.0, 18.0, 84.0, 69.0, 54.0, 138.0, 114.0, 90.0) [expect-fail]

mat3 test_mat3_multiply_zero() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 zero = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return a * zero;
}

// run: test_mat3_multiply_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_multiply_variables() {
    mat3 a = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // identity
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    return a * b;
}

// run: test_mat3_multiply_variables() ~= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0)

mat3 test_mat3_multiply_expressions() {
    return mat3(1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0) * mat3(1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0);
    // Result depends on matrix multiplication
}

// run: test_mat3_multiply_expressions() ~= mat3(2.0, 1.0, 1.0, 1.0, 2.0, 1.0, 0.0, 1.0, 1.0) [expect-fail]

mat3 test_mat3_multiply_in_assignment() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result = result * mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // multiply by identity
    return result;
}

// run: test_mat3_multiply_in_assignment() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_multiply_commutative() {
    // Test that matrix multiplication is not generally commutative
    mat3 a = mat3(1.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    mat3 b = mat3(1.0, 0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return (a * b) - (b * a); // Should not be zero matrix
    // This will show that A*B != B*A for these matrices
}

// run: test_mat3_multiply_commutative() ~= mat3(0.0, 2.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]
