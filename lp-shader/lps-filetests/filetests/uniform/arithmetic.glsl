// test run

// ============================================================================
// Uniform: Arithmetic with uniforms (default zero values)
// ============================================================================

layout(binding = 0) uniform float u_time;
layout(binding = 0) uniform vec2 u_offset;

float test_uniform_add_constant() {
    return u_time + 1.0;
}

// @unimplemented(jit.q32)
// run: test_uniform_add_constant() ~= 1.0

vec2 test_uniform_vec_offset() {
    return u_offset + vec2(10.0, 20.0);
}

// @unimplemented(jit.q32)
// run: test_uniform_vec_offset() ~= vec2(10.0, 20.0)

float test_uniform_multiply() {
    return u_time * 2.0 + 5.0;
}

// @unimplemented(jit.q32)
// run: test_uniform_multiply() ~= 5.0
