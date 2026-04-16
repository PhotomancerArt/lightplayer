// test run

// ============================================================================
// Uniform Global Declarations: Global variables with uniform qualifier
// ============================================================================
// Single UBO with explicit std430 layout (matches LPVM); members are in global scope.

layout(std430, binding = 0) uniform Decl {
    float time;
    int frame_count;
    uint seed;
    bool enabled;
    vec2 resolution;
    vec3 camera_position;
    vec4 color;
    mat2 transform_2d;
    mat3 transform_3d;
    mat4 model_view_projection;
};

float test_declare_uniform_float() {
    return time * 2.0;
}

// run: test_declare_uniform_float() ~= 0.0

int test_declare_uniform_int() {
    return frame_count + 1;
}

// run: test_declare_uniform_int() == 1

uint test_declare_uniform_uint() {
    return seed / 2u;
}

// run: test_declare_uniform_uint() == 0u

bool test_declare_uniform_bool() {
    return enabled;
}

// run: test_declare_uniform_bool() == false

vec2 test_declare_uniform_vec2() {
    return resolution * 0.5;
}

// run: test_declare_uniform_vec2() ~= vec2(0.0, 0.0)

vec3 test_declare_uniform_vec3() {
    return camera_position + vec3(1.0, 0.0, 0.0);
}

// run: test_declare_uniform_vec3() ~= vec3(1.0, 0.0, 0.0)

vec4 test_declare_uniform_vec4() {
    return color;
}

// run: test_declare_uniform_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0)

mat2 test_declare_uniform_mat2() {
    return transform_2d;
}

// run: test_declare_uniform_mat2() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat3 test_declare_uniform_mat3() {
    return transform_3d;
}

// run: test_declare_uniform_mat3() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_declare_uniform_mat4() {
    return model_view_projection;
}

// run: test_declare_uniform_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
