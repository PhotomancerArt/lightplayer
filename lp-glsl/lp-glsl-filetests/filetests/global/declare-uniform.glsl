// test run
// target riscv32.q32

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

// run: test_declare_uniform_float() ~= 0.0 [expect-fail]

int test_declare_uniform_int() {
    // Uniform global int declaration
    return frame_count + 1;
}

// run: test_declare_uniform_int() == 1 [expect-fail]

uint test_declare_uniform_uint() {
    // Uniform global uint declaration
    return int(seed / 2u);
}

// run: test_declare_uniform_uint() == 0 [expect-fail]

bool test_declare_uniform_bool() {
    // Uniform global bool declaration
    return enabled;
}

// run: test_declare_uniform_bool() == false [expect-fail]

vec2 test_declare_uniform_vec2() {
    // Uniform global vec2 declaration
    return resolution * 0.5;
}

// run: test_declare_uniform_vec2() ~= vec2(0.0, 0.0) [expect-fail]

vec3 test_declare_uniform_vec3() {
    // Uniform global vec3 declaration
    return camera_position + vec3(1.0, 0.0, 0.0);
}

// run: test_declare_uniform_vec3() ~= vec3(1.0, 0.0, 0.0) [expect-fail]

vec4 test_declare_uniform_vec4() {
    // Uniform global vec4 declaration
    return color;
}

// run: test_declare_uniform_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat2 test_declare_uniform_mat2() {
    // Uniform global mat2 declaration
    return transform_2d;
}

// run: test_declare_uniform_mat2() ~= mat2(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat3 test_declare_uniform_mat3() {
    // Uniform global mat3 declaration
    vec3 col0 = transform_3d[0];
    return transform_3d;
}

// run: test_declare_uniform_mat3() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

mat4 test_declare_uniform_mat4() {
    // Uniform global mat4 declaration
    return model_view_projection;
}

// run: test_declare_uniform_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]
