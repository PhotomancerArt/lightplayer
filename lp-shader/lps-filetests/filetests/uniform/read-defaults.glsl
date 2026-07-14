// test run

// ============================================================================
// Uniform: Read default (zero) values
// ============================================================================
// All uniforms are zero-initialized by default when no set_uniform is used.

layout(binding = 0) uniform float u_time;
layout(binding = 0) uniform int u_frame;
layout(binding = 0) uniform vec2 u_resolution;
layout(binding = 0) uniform vec3 u_camera;
layout(binding = 0) uniform vec4 u_color;

float test_uniform_float_default() {
    return u_time;
}

// run: test_uniform_float_default() ~= 0.0

int test_uniform_int_default() {
    return u_frame;
}

// run: test_uniform_int_default() == 0

vec2 test_uniform_vec2_default() {
    return u_resolution;
}

// run: test_uniform_vec2_default() ~= vec2(0.0, 0.0)

vec4 test_uniform_vec4_default() {
    return u_color;
}

// run: test_uniform_vec4_default() ~= vec4(0.0, 0.0, 0.0, 0.0)
