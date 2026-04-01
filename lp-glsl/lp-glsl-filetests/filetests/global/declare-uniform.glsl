// test run

// ============================================================================
// Uniform Global Declarations: Global variables with uniform qualifier
// ============================================================================

uniform float time;
uniform int frame_count;
uniform uint seed;
uniform bool enabled;
uniform vec2 resolution;
uniform vec3 camera_position;
uniform vec4 color;
uniform mat2 transform_2d;
uniform mat3 transform_3d;
uniform mat4 model_view_projection;

float test_declare_uniform_float() {
    // Uniform global float declaration
    // Note: uniforms are read-only, cannot assign
    return time * 2.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_float() ~= 0.0

int test_declare_uniform_int() {
    // Uniform global int declaration
    return frame_count + 1;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_int() == 1

uint test_declare_uniform_uint() {
    // Uniform global uint declaration
    return int(seed / 2u);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_uint() == 0

bool test_declare_uniform_bool() {
    // Uniform global bool declaration
    return enabled;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_bool() == false

vec2 test_declare_uniform_vec2() {
    // Uniform global vec2 declaration
    return resolution * 0.5;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_vec2() ~= vec2(0.0, 0.0)

vec3 test_declare_uniform_vec3() {
    // Uniform global vec3 declaration
    return camera_position + vec3(1.0, 0.0, 0.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_vec3() ~= vec3(1.0, 0.0, 0.0)

vec4 test_declare_uniform_vec4() {
    // Uniform global vec4 declaration
    return color;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0)

mat2 test_declare_uniform_mat2() {
    // Uniform global mat2 declaration
    return transform_2d;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_mat2() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat3 test_declare_uniform_mat3() {
    // Uniform global mat3 declaration
    vec3 col0 = transform_3d[0];
    return transform_3d;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_mat3() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_declare_uniform_mat4() {
    // Uniform global mat4 declaration
    return model_view_projection;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_declare_uniform_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
