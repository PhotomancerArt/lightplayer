// test run

// ============================================================================
// Global Variable Read Access: Reading from global variables
// ============================================================================

// Unsupported by design on the lps-glsl targets: LightPlayer shaders have no
// pipeline stage and no storage, so lps-glsl rejects `in`/`out` globals,
// `buffer` blocks and `shared` variables at global scope.

float global_float = 42.0;
int global_int = -123;
uint global_uint = 987u;
bool global_bool = true;
vec2 global_vec2 = vec2(1.0, 2.0);
vec3 global_vec3 = vec3(1.0, 2.0, 3.0);
vec4 global_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
mat2 global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);

const float CONST_FLOAT = 3.14;
uniform float uniform_time;
in vec3 in_position;
buffer DataBlock { float buffer_data[4]; };

float test_access_read_float() {
    // Reading global float
    return global_float;
}

// @unsupported(*)
// run: test_access_read_float() ~= 42.0

int test_access_read_int() {
    // Reading global int
    return global_int;
}

// @unsupported(*)
// run: test_access_read_int() == -123

uint test_access_read_uint() {
    // Reading global uint
    return int(global_uint);
}

// @unsupported(*)
// run: test_access_read_uint() == 987

bool test_access_read_bool() {
    // Reading global bool
    return global_bool;
}

// @unsupported(*)
// run: test_access_read_bool() == true

vec2 test_access_read_vec2() {
    // Reading global vec2
    return global_vec2;
}

// @unsupported(*)
// run: test_access_read_vec2() ~= vec2(1.0, 2.0)

vec3 test_access_read_vec3() {
    // Reading global vec3
    return global_vec3;
}

// @unsupported(*)
// run: test_access_read_vec3() ~= vec3(1.0, 2.0, 3.0)

vec4 test_access_read_vec4() {
    // Reading global vec4
    return global_vec4;
}

// @unsupported(*)
// run: test_access_read_vec4() ~= vec4(1.0, 2.0, 3.0, 4.0)

mat2 test_access_read_mat2() {
    // Reading global mat2
    return global_mat2;
}

// @unsupported(*)
// run: test_access_read_mat2() ~= mat2(1.0, 2.0, 3.0, 4.0)

float test_access_read_const() {
    // Reading const global
    return CONST_FLOAT * 2.0;
}

// @unsupported(*)
// run: test_access_read_const() ~= 6.28

float test_access_read_uniform() {
    // Reading uniform global
    return uniform_time + 1.0;
}

// @unsupported(*)
// run: test_access_read_uniform() ~= 1.0

vec3 test_access_read_in() {
    // Reading in global
    return in_position + vec3(1.0, 1.0, 1.0);
}

// @unsupported(*)
// run: test_access_read_in() ~= vec3(1.0, 1.0, 1.0)

float test_access_read_buffer() {
    // Reading buffer global
    buffer_data[0] = 5.0;
    buffer_data[1] = 10.0;
    return buffer_data[0] + buffer_data[1];
}

// wgpu.f32: file does not compile through naga glsl-in
// @unsupported(*)
// run: test_access_read_buffer() ~= 15.0
