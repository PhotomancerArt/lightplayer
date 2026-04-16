// test run

// ============================================================================
// In Read-Only: Input global variables are read-only in shader code
// ============================================================================

in float vertex_time;
in int vertex_id;
in uint instance_id;
in bool vertex_selected;
in vec2 texture_coord;
in vec3 vertex_position;
in vec4 vertex_color;

float test_in_readonly_float() {
    // Input float is read-only - can only read, not write
    return vertex_time + 1.0;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_float() ~= 1.0

int test_in_readonly_int() {
    // Input int is read-only
    return vertex_id + 100;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_int() == 100

uint test_in_readonly_uint() {
    // Input uint is read-only
    return int(instance_id + 50u);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_in_readonly_uint() == 50

bool test_in_readonly_bool() {
    // Input bool is read-only
    return vertex_selected;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_bool() == false

vec2 test_in_readonly_vec2() {
    // Input vec2 is read-only
    return texture_coord * 2.0;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_vec2() ~= vec2(0.0, 0.0)

vec3 test_in_readonly_vec3() {
    // Input vec3 is read-only
    return vertex_position + vec3(0.0, 1.0, 0.0);
}

// @unimplemented(jit.q32)
// run: test_in_readonly_vec3() ~= vec3(0.0, 1.0, 0.0)

vec4 test_in_readonly_vec4() {
    // Input vec4 is read-only
    return vertex_color;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0)

float test_in_readonly_calculations() {
    // Input variables used in vertex calculations
    float time_factor = vertex_time * 2.0;
    vec2 scaled_texcoord = texture_coord * 0.5;
    vec3 offset_position = vertex_position + vec3(0.1, 0.1, 0.1);

    return time_factor + scaled_texcoord.x + scaled_texcoord.y +
           offset_position.x + offset_position.y + offset_position.z;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_calculations() ~= 0.3

vec4 test_in_readonly_vertex_processing() {
    // Input variables in vertex processing
    vec4 processed_color = vertex_color;

    if (vertex_selected) {
        processed_color = processed_color + vec4(0.5, 0.5, 0.5, 0.0);
    }

    processed_color.a = float(vertex_id) / 100.0;

    return processed_color;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_vertex_processing() ~= vec4(0.0, 0.0, 0.0, 0.0)

float test_in_readonly_texture_mapping() {
    // Input texture coordinates for mapping
    vec2 uv = texture_coord;
    float u = uv.x;
    float v = uv.y;

    // Simple texture coordinate manipulation
    u = u * 2.0 - 1.0;  // Convert to -1..1 range
    v = v * 2.0 - 1.0;

    return u + v;
}

// @unimplemented(jit.q32)
// run: test_in_readonly_texture_mapping() ~= -2.0
