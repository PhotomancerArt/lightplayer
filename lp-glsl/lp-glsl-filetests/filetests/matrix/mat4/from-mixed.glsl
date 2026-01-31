// test run
// target riscv32.q32

// ============================================================================
// From Mixed: mat4 constructors with mixed types - must provide exactly 16 components
// Per GLSL spec: "there must be enough components provided in the arguments to
// provide an initializer for every component in the constructed value"
// ============================================================================

// Valid: mat4(vec4, vec4, vec4, vec4) - 4+4+4+4 = 16 components
mat4 test_mat4_from_vec4_vec4_vec4_vec4() {
    // Constructor mat4(vec4, vec4, vec4, vec4) - each vec4 becomes a column
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0));
}

// run: test_mat4_from_vec4_vec4_vec4_vec4() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec4, vec4, vec4, vec3, float) - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_vec4_vec4_vec4_vec3_float() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec3(13.0, 14.0, 15.0), 16.0);
}

// run: test_mat4_from_vec4_vec4_vec4_vec3_float() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec4, vec4, vec2, vec2, vec2, vec2) - 4+4+2+2+2+2 = 16 components
mat4 test_mat4_from_vec4_vec4_vec2_vec2_vec2_vec2() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec2(9.0, 10.0), vec2(11.0, 12.0), vec2(13.0, 14.0), vec2(15.0, 16.0));
}

// run: test_mat4_from_vec4_vec4_vec2_vec2_vec2_vec2() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec4, vec3, vec3, vec3, vec3) - 4+3+3+3+3 = 16 components
mat4 test_mat4_from_vec4_vec3_vec3_vec3_vec3() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec3(5.0, 6.0, 7.0), vec3(8.0, 9.0, 10.0), vec3(11.0, 12.0, 13.0), vec3(14.0, 15.0, 16.0));
}

// run: test_mat4_from_vec4_vec3_vec3_vec3_vec3() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec3, vec3, vec3, vec3, vec2, vec2) - 3+3+3+3+2+2 = 16 components
mat4 test_mat4_from_vec3_vec3_vec3_vec3_vec2_vec2() {
    return mat4(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0), vec3(10.0, 11.0, 12.0), vec2(13.0, 14.0), vec2(15.0, 16.0));
}

// run: test_mat4_from_vec3_vec3_vec3_vec3_vec2_vec2() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec4, vec4, vec4, float, float, float, float) - 4+4+4+1+1+1+1 = 16 components
mat4 test_mat4_from_vec4_vec4_vec4_float4() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), 13.0, 14.0, 15.0, 16.0);
}

// run: test_mat4_from_vec4_vec4_vec4_float4() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec4, vec2, vec2, vec2, vec2, vec2, vec2) - 4+2+2+2+2+2+2 = 16 components
mat4 test_mat4_from_vec4_vec2_vec2_vec2_vec2_vec2_vec2() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec2(5.0, 6.0), vec2(7.0, 8.0), vec2(9.0, 10.0), vec2(11.0, 12.0), vec2(13.0, 14.0), vec2(15.0, 16.0));
}

// run: test_mat4_from_vec4_vec2_vec2_vec2_vec2_vec2_vec2() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4(vec2, vec2, vec2, vec2, vec2, vec2, vec2, vec2) - 2+2+2+2+2+2+2+2 = 16 components
mat4 test_mat4_from_vec2_vec2_vec2_vec2_vec2_vec2_vec2_vec2() {
    return mat4(vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0), vec2(7.0, 8.0), vec2(9.0, 10.0), vec2(11.0, 12.0), vec2(13.0, 14.0), vec2(15.0, 16.0));
}

// run: test_mat4_from_vec2_vec2_vec2_vec2_vec2_vec2_vec2_vec2() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4 from 16 scalars - 16 components
mat4 test_mat4_from_multiple_scalars() {
    return mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
}

// run: test_mat4_from_multiple_scalars() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4 from mixed expressions - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_mixed_expressions() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0) + vec4(0.5, 0.5, 0.5, 0.5), vec4(5.0, 6.0, 7.0, 8.0) * vec4(2.0, 2.0, 2.0, 2.0), vec4(9.0, 10.0, 11.0, 12.0), vec3(13.0, 14.0, 15.0), 16.0);
}

// run: test_mat4_from_mixed_expressions() ~= mat4(1.5, 2.5, 3.5, 4.5, 10.0, 12.0, 14.0, 16.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4 from mixed variables - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_mixed_variables() {
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 w = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 x = vec4(9.0, 10.0, 11.0, 12.0);
    vec3 y = vec3(13.0, 14.0, 15.0);
    float a = 16.0;
    return mat4(v, w, x, y, a);
}

// run: test_mat4_from_mixed_variables() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// Valid: mat4 from mixed negative values - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_mixed_negative() {
    return mat4(vec4(-1.0, -2.0, -3.0, -4.0), vec4(-5.0, -6.0, -7.0, -8.0), vec4(-9.0, -10.0, -11.0, -12.0), vec3(-13.0, -14.0, -15.0), -16.0);
}

// run: test_mat4_from_mixed_negative() ~= mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0)

// Valid: mat4 from mixed zero values - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_mixed_zero() {
    return mat4(vec4(0.0, 0.0, 0.0, 0.0), vec4(0.0, 0.0, 0.0, 0.0), vec4(0.0, 0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0), 0.0);
}

// run: test_mat4_from_mixed_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

// Valid: mat4 from mixed in assignment - 4+4+4+3+1 = 16 components
mat4 test_mat4_from_mixed_in_assignment() {
    mat4 result;
    result = mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec3(13.0, 14.0, 15.0), 16.0);
    return result;
}

// run: test_mat4_from_mixed_in_assignment() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

// ============================================================================
// Invalid cases: These should fail compilation per GLSL spec
// Note: These functions have no // run: directives - they should fail to compile
// ============================================================================

// TODO: Add proper error expectations
// Invalid: mat4(vec4, vec4, vec2, float) - only 11 components, needs 16
// This should produce a compilation error
// mat4 test_mat4_invalid_too_few_components() {
//     return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec2(9.0, 10.0), 11.0);
// }

// TODO: Add proper error expectations
// Invalid: mat4(vec4, vec4, vec4) - only 12 components, needs 16
// This should produce a compilation error
// mat4 test_mat4_invalid_only_three_vec4() {
//     return mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0));
// }




