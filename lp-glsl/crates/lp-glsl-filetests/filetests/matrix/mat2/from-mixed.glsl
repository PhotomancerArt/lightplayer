// test run
// target riscv32.q32

// ============================================================================
// From Mixed: mat2(vec2, float, float) - construct matrix from mixed types
// ============================================================================

mat2 test_mat2_from_vec2_scalar_scalar() {
    // Constructor mat2(vec2, float, float) - vec2 is first column, scalars fill remaining
    return mat2(vec2(1.0, 2.0), 3.0, 4.0);
}

// run: test_mat2_from_vec2_scalar_scalar() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_scalar_vec2_scalar() {
    return mat2(1.0, vec2(2.0, 3.0), 4.0);
}

// run: test_mat2_from_scalar_vec2_scalar() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_scalar_scalar_vec2() {
    return mat2(1.0, 2.0, vec2(3.0, 4.0));
}

// run: test_mat2_from_scalar_scalar_vec2() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_multiple_scalars() {
    return mat2(1.0, 2.0, 3.0, 4.0);
}

// run: test_mat2_from_multiple_scalars() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_mixed_expressions() {
    return mat2(vec2(1.0, 2.0) + vec2(0.5, 0.5), 3.0 * 2.0, 4.0 + 1.0);
}

// run: test_mat2_from_mixed_expressions() ~= mat2(1.5, 2.5, 6.0, 5.0)

mat2 test_mat2_from_mixed_variables() {
    vec2 v = vec2(1.0, 2.0);
    float a = 3.0, b = 4.0;
    return mat2(v, a, b);
}

// run: test_mat2_from_mixed_variables() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_mixed_negative() {
    return mat2(vec2(-1.0, -2.0), -3.0, -4.0);
}

// run: test_mat2_from_mixed_negative() ~= mat2(-1.0, -2.0, -3.0, -4.0)

mat2 test_mat2_from_mixed_zero() {
    return mat2(vec2(0.0, 0.0), 0.0, 0.0);
}

// run: test_mat2_from_mixed_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_from_mixed_in_assignment() {
    mat2 result;
    result = mat2(vec2(1.0, 2.0), 3.0, 4.0);
    return result;
}

// run: test_mat2_from_mixed_in_assignment() ~= mat2(1.0, 2.0, 3.0, 4.0)




