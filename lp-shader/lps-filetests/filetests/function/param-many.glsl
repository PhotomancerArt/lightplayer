// compile-opt(inline.mode, never)

// test run

// ============================================================================
// Many Parameters: Tests stack parameter passing (>8 slots)
// ============================================================================

// 10 scalar parameters (vmctx + 9 user = 10 slots, 2 on stack)
float sum_nine_scalars(
    float a, float b, float c, float d, float e,
    float f, float g, float h, float i
) {
    return a + b + c + d + e + f + g + h + i;
}

float test_many_scalars() {
    return sum_nine_scalars(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
}

// run: test_many_scalars() ~= 45.0

// float + mat2 = 1 + 4 = 5 slots (all in regs)
float scalar_plus_mat2(float s, mat2 m) {
    return s + m[0][0] + m[1][1];
}

float test_scalar_plus_mat2() {
    return scalar_plus_mat2(10.0, mat2(1.0, 0.0, 0.0, 2.0));
}

// run: test_scalar_plus_mat2() ~= 13.0

// float + mat3 = 1 + 9 = 10 slots (vmctx + 9 user = 2 on stack)
float scalar_plus_mat3(float s, mat3 m) {
    return s + m[0][0] + m[2][2];
}

float test_scalar_plus_mat3() {
    return scalar_plus_mat3(10.0, mat3(
        1.0, 0.0, 0.0,
        0.0, 1.0, 0.0,
        0.0, 0.0, 1.0
    ));
}

// run: test_scalar_plus_mat3() ~= 12.0

// mat4 alone = 16 slots (vmctx + 16 = 8 on stack)
float mat4_trace(mat4 m) {
    return m[0][0] + m[1][1] + m[2][2] + m[3][3];
}

float test_mat4_trace() {
    return mat4_trace(mat4(
        1.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 3.0, 0.0,
        0.0, 0.0, 0.0, 4.0
    ));
}

// run: test_mat4_trace() ~= 10.0

// Two mat2s = 8 scalars + vmctx = 9 slots (1 on stack)
// This matches the call-nested.glsl case
mat2 combine_mat2(mat2 a, mat2 b) {
    return a + b;
}

float test_combine_mat2() {
    mat2 result = combine_mat2(
        mat2(1.0, 0.0, 0.0, 1.0),
        mat2(2.0, 0.0, 0.0, 3.0)
    );
    return result[0][0] + result[1][1];
}

// run: test_combine_mat2() ~= 7.0

// Mixed: vec2 + vec3 + float = 2 + 3 + 1 = 6 slots (all in regs)
float mixed_vectors(vec2 a, vec3 b, float c) {
    return a.x + b.y + c;
}

float test_mixed_vectors_in_regs() {
    return mixed_vectors(vec2(1.0, 2.0), vec3(3.0, 4.0, 5.0), 6.0);
}

// run: test_mixed_vectors_in_regs() ~= 11.0

// Many params: vec4 + vec4 + float = 4 + 4 + 1 = 9 slots (1 on stack)
float many_vector_params(vec4 a, vec4 b, float c) {
    return a.x + b.w + c;
}

float test_many_vector_params() {
    return many_vector_params(
        vec4(1.0, 2.0, 3.0, 4.0),
        vec4(5.0, 6.0, 7.0, 8.0),
        9.0
    );
}

// run: test_many_vector_params() ~= 18.0

// Nested call with many params: inner has 9 slots, outer has 5
float inner_many(float a, float b, float c, float d, float e, float f, float g, float h, float i) {
    return a * i; // First * last
}

float outer_many(float x) {
    return inner_many(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) + x;
}

float test_nested_many_params() {
    return outer_many(10.0);
}

// run: test_nested_many_params() ~= 19.0

// Chain of calls passing through many params
float pass_through(float a, float b, float c, float d, float e, float f, float g, float h) {
    return a + h;
}

float chain_a(float v) {
    return pass_through(v, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0);
}

float chain_b(float v) {
    return chain_a(v);
}

float test_param_chain() {
    return chain_b(1.0);
}

// run: test_param_chain() ~= 9.0
