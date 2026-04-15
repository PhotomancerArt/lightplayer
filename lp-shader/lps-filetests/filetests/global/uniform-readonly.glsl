// test run

// ============================================================================
// Uniform Read-Only: Uniform global variables are read-only in shader code
// ============================================================================

uniform float time;
uniform int frame_count;
uniform uint random_seed;
uniform bool enable_lighting;
uniform vec2 resolution;
uniform vec3 camera_position;
uniform vec4 ambient_color;
uniform mat4 view_matrix;
uniform mat3 normal_matrix;

float test_uniform_readonly_float() {
    // Uniform float is read-only - can only read, not write
    return time + 1.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_float() ~= 1.0

int test_uniform_readonly_int() {
    // Uniform int is read-only
    return frame_count * 2;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_int() == 0

uint test_uniform_readonly_uint() {
    // Uniform uint is read-only
    return int(random_seed + 1u);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_uint() == 1

bool test_uniform_readonly_bool() {
    // Uniform bool is read-only
    return enable_lighting;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_bool() == false

vec2 test_uniform_readonly_vec2() {
    // Uniform vec2 is read-only
    return resolution * 0.5;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_vec2() ~= vec2(0.0, 0.0)

vec3 test_uniform_readonly_vec3() {
    // Uniform vec3 is read-only
    return camera_position + vec3(0.0, 1.0, 0.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_vec3() ~= vec3(0.0, 1.0, 0.0)

vec4 test_uniform_readonly_vec4() {
    // Uniform vec4 is read-only
    return ambient_color;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0)

mat4 test_uniform_readonly_mat4() {
    // Uniform mat4 is read-only
    return view_matrix;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_uniform_readonly_mat3() {
    // Uniform mat3 is read-only
    return normal_matrix;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_mat3() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

float test_uniform_readonly_calculations() {
    // Uniform variables used in calculations
    float scaled_time = time * 2.0;
    vec2 half_resolution = resolution * 0.5;
    vec3 elevated_camera = camera_position + vec3(0.0, 10.0, 0.0);

    return scaled_time + half_resolution.x + half_resolution.y + elevated_camera.y;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_calculations() ~= 10.0

vec4 test_uniform_readonly_lighting() {
    // Uniform variables in lighting calculations
    vec4 lighting_color = ambient_color;

    if (enable_lighting) {
        lighting_color = lighting_color + vec4(0.5, 0.5, 0.5, 0.0);
    }

    return lighting_color;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_lighting() ~= vec4(0.0, 0.0, 0.0, 0.0)

float test_uniform_readonly_transform() {
    // Uniform matrices in transformations
    vec4 position = vec4(1.0, 2.0, 3.0, 1.0);
    vec4 transformed = view_matrix * position;

    return transformed.x + transformed.y + transformed.z + transformed.w;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_uniform_readonly_transform() ~= 0.0
