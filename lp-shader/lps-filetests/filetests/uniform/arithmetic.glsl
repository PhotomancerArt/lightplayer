// test run

// ============================================================================
// Uniform: Arithmetic with uniforms (default zero values)
// ============================================================================

uniform float u_time;
uniform vec2 u_offset;

float test_uniform_add_constant() {
    return u_time + 1.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_add_constant() ~= 1.0

vec2 test_uniform_vec_offset() {
    return u_offset + vec2(10.0, 20.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_vec_offset() ~= vec2(10.0, 20.0)

float test_uniform_multiply() {
    return u_time * 2.0 + 5.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_multiply() ~= 5.0
