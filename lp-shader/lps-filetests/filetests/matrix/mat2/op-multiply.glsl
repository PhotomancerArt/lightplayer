// test run

// ============================================================================
// Multiply: mat2 * mat2 -> mat2 (matrix multiplication)
// ============================================================================

mat2 test_mat2_multiply_identity() {
    // Matrix multiplication with identity matrix
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 identity = mat2(1.0, 0.0, 0.0, 1.0);
    return a * identity;
}

// run: test_mat2_multiply_identity() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_multiply_simple() {
    // Simple matrix multiplication
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(5.0, 6.0, 7.0, 8.0);
    // GLSL column-major: mat2(1,2,3,4) has columns (1,2) and (3,4); product uses linear algebra.
    return a * b;
}

// run: test_mat2_multiply_simple() ~= mat2(23.0, 34.0, 31.0, 46.0)

mat2 test_mat2_multiply_scale() {
    // Scaling matrix multiplication
    mat2 a = mat2(2.0, 0.0, 0.0, 3.0); // scale matrix
    mat2 b = mat2(1.0, 2.0, 3.0, 4.0);
    return a * b;
}

// run: test_mat2_multiply_scale() ~= mat2(2.0, 6.0, 6.0, 12.0)

mat2 test_mat2_multiply_zero() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 zero = mat2(0.0, 0.0, 0.0, 0.0);
    return a * zero;
}

// run: test_mat2_multiply_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_multiply_variables() {
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0); // identity
    mat2 b = mat2(2.0, 3.0, 4.0, 5.0);
    return a * b;
}

// run: test_mat2_multiply_variables() ~= mat2(2.0, 3.0, 4.0, 5.0)

mat2 test_mat2_multiply_expressions() {
    return mat2(1.0, 1.0, 0.0, 1.0) * mat2(1.0, 0.0, 1.0, 1.0);
}

// run: test_mat2_multiply_expressions() ~= mat2(1.0, 1.0, 1.0, 2.0)

mat2 test_mat2_multiply_in_assignment() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result = result * mat2(1.0, 0.0, 0.0, 1.0); // multiply by identity
    return result;
}

// run: test_mat2_multiply_in_assignment() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_multiply_associative() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(2.0, 0.0, 0.0, 2.0);
    mat2 c = mat2(1.0, 1.0, 1.0, 1.0);
    return (a * b) * c;
    // (a * b) * c result depends on the matrices
}

// run: test_mat2_multiply_associative() ~= mat2(8.0, 12.0, 8.0, 12.0)
