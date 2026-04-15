// test run

// ============================================================================
// Uniform: Shader with only uniforms (no mutable globals)
// ============================================================================
// Verifies no snapshot/reset overhead — only uniform reads.

uniform float u_brightness;
uniform vec3 u_base_color;

float test_uniforms_only_scalar() {
    return u_brightness + 1.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniforms_only_scalar() ~= 1.0

vec3 test_uniforms_only_vec() {
    return u_base_color + vec3(0.1, 0.2, 0.3);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniforms_only_vec() ~= vec3(0.1, 0.2, 0.3)
