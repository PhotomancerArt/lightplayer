// test run

// ============================================================================
// Uniform: Read default (zero) values
// ============================================================================
// All uniforms are zero-initialized by default when no set_uniform is used.

uniform float u_time;
uniform int u_frame;
uniform vec2 u_resolution;
uniform vec3 u_camera;
uniform vec4 u_color;

float test_uniform_float_default() {
    return u_time;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_float_default() ~= 0.0

int test_uniform_int_default() {
    return u_frame;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_int_default() == 0

vec2 test_uniform_vec2_default() {
    return u_resolution;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_vec2_default() ~= vec2(0.0, 0.0)

vec4 test_uniform_vec4_default() {
    return u_color;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_vec4_default() ~= vec4(0.0, 0.0, 0.0, 0.0)
