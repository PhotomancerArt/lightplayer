// test run
// target riscv32.q32

// ============================================================================
// From Mixed: mat3 constructors with mixed types - must provide exactly 9 components
// Per GLSL spec: "there must be enough components provided in the arguments to
// provide an initializer for every component in the constructed value"
// ============================================================================

// Valid: mat3(vec3, vec3, vec3) - 3+3+3 = 9 components
mat3 test_mat3_from_vec3_vec3_vec3() {
    // Constructor mat3(vec3, vec3, vec3) - each vec3 becomes a column
    return mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
}

// run: test_mat3_from_vec3_vec3_vec3() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3(vec3, vec3, vec2, float) - 3+3+2+1 = 9 components
mat3 test_mat3_from_vec3_vec3_vec2_float() {
    return mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec2(7.0, 8.0), 9.0);
}

// run: test_mat3_from_vec3_vec3_vec2_float() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3(vec3, vec2, vec2, vec2) - 3+2+2+2 = 9 components
mat3 test_mat3_from_vec3_vec2_vec2_vec2() {
    return mat3(vec3(1.0, 2.0, 3.0), vec2(4.0, 5.0), vec2(6.0, 7.0), vec2(8.0, 9.0));
}

// run: test_mat3_from_vec3_vec2_vec2_vec2() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3(vec2, vec2, vec2, vec2, float) - 2+2+2+2+1 = 9 components
mat3 test_mat3_from_vec2_vec2_vec2_vec2_float() {
    return mat3(vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0), vec2(7.0, 8.0), 9.0);
}

// run: test_mat3_from_vec2_vec2_vec2_vec2_float() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3(vec2, vec2, vec2, float, float, float) - 2+2+2+1+1+1 = 9 components
mat3 test_mat3_from_vec2_vec2_vec2_float3() {
    return mat3(vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0), 7.0, 8.0, 9.0);
}

// run: test_mat3_from_vec2_vec2_vec2_float3() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3(vec3, vec3, float, float, float) - 3+3+1+1+1 = 9 components
mat3 test_mat3_from_vec3_vec3_float3() {
    return mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), 7.0, 8.0, 9.0);
}

// run: test_mat3_from_vec3_vec3_float3() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3 from 9 scalars - 9 components
mat3 test_mat3_from_multiple_scalars() {
    return mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
}

// run: test_mat3_from_multiple_scalars() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3 from mixed expressions - 3+3+2+1 = 9 components
mat3 test_mat3_from_mixed_expressions() {
    return mat3(vec3(1.0, 2.0, 3.0) + vec3(0.5, 0.5, 0.5), vec3(4.0, 5.0, 6.0), vec2(7.0, 8.0), 9.0);
}

// run: test_mat3_from_mixed_expressions() ~= mat3(1.5, 2.5, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3 from mixed variables - 3+3+2+1 = 9 components
mat3 test_mat3_from_mixed_variables() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    vec3 w = vec3(4.0, 5.0, 6.0);
    vec2 x = vec2(7.0, 8.0);
    float a = 9.0;
    return mat3(v, w, x, a);
}

// run: test_mat3_from_mixed_variables() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// Valid: mat3 from mixed negative values - 3+3+2+1 = 9 components
mat3 test_mat3_from_mixed_negative() {
    return mat3(vec3(-1.0, -2.0, -3.0), vec3(-4.0, -5.0, -6.0), vec2(-7.0, -8.0), -9.0);
}

// run: test_mat3_from_mixed_negative() ~= mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0)

// Valid: mat3 from mixed zero values - 3+3+2+1 = 9 components
mat3 test_mat3_from_mixed_zero() {
    return mat3(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0), vec2(0.0, 0.0), 0.0);
}

// run: test_mat3_from_mixed_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

// Valid: mat3 from mixed in assignment - 3+3+2+1 = 9 components
mat3 test_mat3_from_mixed_in_assignment() {
    mat3 result;
    result = mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec2(7.0, 8.0), 9.0);
    return result;
}

// run: test_mat3_from_mixed_in_assignment() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

// ============================================================================
// Invalid cases: These should fail compilation per GLSL spec
// Note: These functions have no // run: directives - they should fail to compile
// ============================================================================

// TODO: Add proper error expectations
// Invalid: mat3(vec3, vec2, float) - only 6 components, needs 9
// This should produce a compilation error
// mat3 test_mat3_invalid_too_few_components() {
//     return mat3(vec3(1.0, 2.0, 3.0), vec2(4.0, 5.0), 6.0);
// }

// TODO: Add proper error expectations
// Invalid: mat3(vec3, vec3) - only 6 components, needs 9
// This should produce a compilation error
// mat3 test_mat3_invalid_only_two_vec3() {
//     return mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
// }




