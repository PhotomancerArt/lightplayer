// test run

// ============================================================================
// Uniform Global Declarations: Global variables with uniform qualifier
// ============================================================================

layout(binding = 0) uniform float time;
layout(binding = 0) uniform int frame_count;
layout(binding = 0) uniform uint seed;
layout(binding = 0) uniform bool enabled;
layout(binding = 0) uniform vec2 resolution;
layout(binding = 0) uniform vec3 camera_position;
layout(binding = 0) uniform vec4 color;
layout(binding = 0) uniform mat2 transform_2d;
layout(binding = 0) uniform mat3 transform_3d;
layout(binding = 0) uniform mat4 model_view_projection;

float test_declare_uniform_float() {
    // Uniform global float declaration
    // Note: uniforms are read-only, cannot assign
    return time * 2.0;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_float() ~= 0.0

int test_declare_uniform_int() {
    // Uniform global int declaration
    return frame_count + 1;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_int() == 1

uint test_declare_uniform_uint() {
    // Uniform global uint declaration
    return int(seed / 2u);
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_uint() == 0

bool test_declare_uniform_bool() {
    // Uniform global bool declaration
    return enabled;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_bool() == false

vec2 test_declare_uniform_vec2() {
    // Uniform global vec2 declaration
    return resolution * 0.5;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_vec2() ~= vec2(0.0, 0.0)

vec3 test_declare_uniform_vec3() {
    // Uniform global vec3 declaration
    return camera_position + vec3(1.0, 0.0, 0.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_vec3() ~= vec3(1.0, 0.0, 0.0)

vec4 test_declare_uniform_vec4() {
    // Uniform global vec4 declaration
    return color;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0)

mat2 test_declare_uniform_mat2() {
    // Uniform global mat2 declaration
    return transform_2d;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_mat2() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat3 test_declare_uniform_mat3() {
    // Uniform global mat3 declaration
    vec3 col0 = transform_3d[0];
    return transform_3d;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_mat3() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_declare_uniform_mat4() {
    // Uniform global mat4 declaration
    return model_view_projection;
}

// @unsupported(jit.q32)
// @unsupported(rv32c.q32)
// @unsupported(wasm.q32)
// @unsupported(rv32n.q32)
// run: test_declare_uniform_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
