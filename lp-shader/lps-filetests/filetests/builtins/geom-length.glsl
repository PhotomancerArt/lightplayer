// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// length(): Vector length (Euclidean norm)
// length(x) returns sqrt(dot(x, x)); for a scalar, abs(x)
//
// Lowers through sqrt: on Q32 backends this exercises the @lpir::sqrt helper
// builtin (lps-glsl emits the Fsqrt op with no import decl — lpvm-wasm
// synthesizes the import; see lpvm-wasm/src/emit/imports.rs).
// ============================================================================

float test_length_float() {
    // length of a scalar is its absolute value
    return length(rt(-2.5));
}

// run: test_length_float() ~= 2.5

float test_length_vec2() {
    // 3-4-5 triangle
    return length(vec2(rt(3.0), rt(4.0)));
}

// run: test_length_vec2() ~= 5.0

float test_length_vec2_zero() {
    return length(vec2(rt(0.0), rt(0.0)));
}

// run: test_length_vec2_zero() ~= 0.0

float test_length_vec2_negative() {
    // sign of components must not matter
    return length(vec2(rt(-3.0), rt(4.0)));
}

// run: test_length_vec2_negative() ~= 5.0

float test_length_vec3() {
    // 1-2-2 gives length 3
    return length(vec3(rt(1.0), rt(2.0), rt(2.0)));
}

// run: test_length_vec3() ~= 3.0

float test_length_vec4() {
    // 1-1-1-1 gives length 2
    return length(vec4(rt(1.0), rt(1.0), rt(1.0), rt(1.0)));
}

// run: test_length_vec4() ~= 2.0

float test_length_unit_x() {
    return length(vec2(rt(1.0), rt(0.0)));
}

// run: test_length_unit_x() ~= 1.0
