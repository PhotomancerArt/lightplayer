// test run

// ============================================================================
// Uniform: Non-zero uniform values via set_uniform
// ============================================================================
// These tests require the // set_uniform: directive (implemented in M4).
// For now they serve as the spec for the directive syntax.

uniform float u_time;
uniform float u_speed;
uniform vec2 u_resolution;

float test_uniform_set_float() {
    return u_time;
}

// set_uniform: u_time = 3.0
// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_set_float() ~= 3.0

float test_uniform_set_multiply() {
    return u_time * u_speed;
}

// set_uniform: u_time = 2.0
// set_uniform: u_speed = 5.0
// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_set_multiply() ~= 10.0

vec2 test_uniform_set_vec2() {
    return u_resolution * 0.5;
}

// set_uniform: u_resolution = vec2(1920.0, 1080.0)
// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_set_vec2() ~= vec2(960.0, 540.0)
