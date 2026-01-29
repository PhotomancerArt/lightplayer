// test run
// target riscv32.q32

// ============================================================================
// outerProduct(): Outer product function
// outerProduct(vec, vec) - outer product of two vectors
// Returns matrix: column vector * row vector
// ============================================================================

mat2 test_outerproduct_vec2() {
    // outerProduct(vec2, vec2) returns mat2
    vec2 a = vec2(1.0, 2.0);
    vec2 b = vec2(3.0, 4.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2() ~= mat2(3.0, 4.0, 6.0, 8.0) [expect-fail]

mat3 test_outerproduct_vec3() {
    // outerProduct(vec3, vec3) returns mat3
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(2.0, 2.0, 2.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3() ~= mat3(2.0, 2.0, 2.0, 4.0, 4.0, 4.0, 6.0, 6.0, 6.0) [expect-fail]

mat4 test_outerproduct_vec4() {
    // outerProduct(vec4, vec4) returns mat4
    vec4 a = vec4(1.0, 1.0, 1.0, 1.0);
    vec4 b = vec4(1.0, 2.0, 3.0, 4.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec4() ~= mat4(1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0) [expect-fail]

mat2x3 test_outerproduct_vec2_vec3() {
    // outerProduct(vec2, vec3) returns mat2x3
    vec2 a = vec2(2.0, 3.0);
    vec3 b = vec3(1.0, 2.0, 3.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2_vec3() ~= mat2x3(2.0, 4.0, 6.0, 3.0, 6.0, 9.0) [expect-fail]

mat3x2 test_outerproduct_vec3_vec2() {
    // outerProduct(vec3, vec2) returns mat3x2
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec2 b = vec2(2.0, 3.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3_vec2() ~= mat3x2(2.0, 3.0, 4.0, 6.0, 6.0, 9.0) [expect-fail]

mat2 test_outerproduct_vec2_negative() {
    vec2 a = vec2(-1.0, 2.0);
    vec2 b = vec2(3.0, -4.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2_negative() ~= mat2(-3.0, 4.0, 6.0, -8.0) [expect-fail]

mat3 test_outerproduct_vec3_negative() {
    vec3 a = vec3(-1.0, 2.0, -3.0);
    vec3 b = vec3(2.0, -2.0, 3.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3_negative() ~= mat3(-2.0, 2.0, -3.0, 4.0, -4.0, 6.0, -6.0, 6.0, -9.0) [expect-fail]

mat4 test_outerproduct_vec4_negative() {
    vec4 a = vec4(-1.0, 1.0, -1.0, 1.0);
    vec4 b = vec4(1.0, -1.0, 1.0, -1.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec4_negative() ~= mat4(-1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0) [expect-fail]

mat2 test_outerproduct_vec2_zero() {
    vec2 a = vec2(1.0, 2.0);
    vec2 b = vec2(0.0, 0.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2_zero() ~= mat2(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat3 test_outerproduct_vec3_zero() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(0.0, 0.0, 0.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

mat4 test_outerproduct_vec4_zero() {
    vec4 a = vec4(1.0, 1.0, 1.0, 1.0);
    vec4 b = vec4(0.0, 0.0, 0.0, 0.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec4_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

mat2 test_outerproduct_vec2_fractions() {
    vec2 a = vec2(0.5, 1.5);
    vec2 b = vec2(2.0, 2.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2_fractions() ~= mat2(1.0, 1.0, 3.0, 3.0) [expect-fail]

mat3 test_outerproduct_vec3_fractions() {
    vec3 a = vec3(0.5, 1.5, 2.5);
    vec3 b = vec3(2.0, 2.0, 2.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3_fractions() ~= mat3(1.0, 1.0, 1.0, 3.0, 3.0, 3.0, 5.0, 5.0, 5.0) [expect-fail]

mat4 test_outerproduct_vec4_fractions() {
    vec4 a = vec4(0.5, 1.5, 2.5, 3.5);
    vec4 b = vec4(2.0, 2.0, 2.0, 2.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec4_fractions() ~= mat4(1.0, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0, 3.0, 5.0, 5.0, 5.0, 5.0, 7.0, 7.0, 7.0, 7.0) [expect-fail]

mat2 test_outerproduct_vec2_expressions() {
    return outerProduct(vec2(2.0, 3.0), vec2(3.0, 4.0));
}

// run: test_outerproduct_vec2_expressions() ~= mat2(6.0, 8.0, 9.0, 12.0) [expect-fail]

mat3 test_outerproduct_vec3_expressions() {
    return outerProduct(vec3(1.0, 2.0, 3.0), vec3(2.0, 3.0, 4.0));
}

// run: test_outerproduct_vec3_expressions() ~= mat3(2.0, 3.0, 4.0, 4.0, 6.0, 8.0, 6.0, 9.0, 12.0) [expect-fail]

mat4 test_outerproduct_vec4_expressions() {
    return outerProduct(vec4(1.0, 1.0, 1.0, 1.0), vec4(1.0, 2.0, 3.0, 4.0));
}

// run: test_outerproduct_vec4_expressions() ~= mat4(1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0) [expect-fail]

mat2 test_outerproduct_vec2_variables() {
    vec2 a = vec2(2.0, 3.0);
    vec2 b = vec2(4.0, 5.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec2_variables() ~= mat2(8.0, 10.0, 12.0, 15.0) [expect-fail]

mat3 test_outerproduct_vec3_variables() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(3.0, 2.0, 1.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec3_variables() ~= mat3(3.0, 2.0, 1.0, 6.0, 4.0, 2.0, 9.0, 6.0, 3.0) [expect-fail]

mat4 test_outerproduct_vec4_variables() {
    vec4 a = vec4(2.0, 2.0, 2.0, 2.0);
    vec4 b = vec4(1.0, 2.0, 3.0, 4.0);
    return outerProduct(a, b);
}

// run: test_outerproduct_vec4_variables() ~= mat4(2.0, 4.0, 6.0, 8.0, 2.0, 4.0, 6.0, 8.0, 2.0, 4.0, 6.0, 8.0, 2.0, 4.0, 6.0, 8.0) [expect-fail]
