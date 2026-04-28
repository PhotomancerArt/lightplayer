// test run

// ============================================================================
// Edge Uniform Write Error: Writing to uniform globals may be a compile error
// ============================================================================

layout(binding = 0) uniform float time;
layout(binding = 0) uniform int count;
layout(binding = 0) uniform vec2 position;
layout(binding = 0) uniform vec3 color;
layout(binding = 0) uniform mat4 transform;

// These would typically be compile errors (uniforms are read-only):
// time = 5.0;                    // Error: cannot assign to uniform
// count = 42;                    // Error: cannot assign to uniform
// position = vec2(1.0, 1.0);     // Error: cannot assign to uniform
// color = vec3(0.5, 0.5, 0.5);  // Error: cannot assign to uniform
// transform = mat4(1.0);        // Error: cannot assign to uniform

// However, reading from uniforms is allowed
float test_edge_uniform_write_error_read() {
    // Reading from uniform is allowed
    return time + 1.0;
}

// @unimplemented(jit.q32)
// run: test_edge_uniform_write_error_read() ~= 1.0

int test_edge_uniform_write_error_int() {
    // Reading uniform int is allowed
    return count + 5;
}

// @unimplemented(jit.q32)
// run: test_edge_uniform_write_error_int() == 5

vec2 test_edge_uniform_write_error_vec2() {
    // Reading uniform vec2 is allowed
    return position * 2.0;
}

// @unimplemented(jit.q32)
// run: test_edge_uniform_write_error_vec2() ~= vec2(0.0, 0.0)

vec3 test_edge_uniform_write_error_vec3() {
    // Reading uniform vec3 is allowed
    return color + vec3(0.1, 0.1, 0.1);
}

// @unimplemented(jit.q32)
// run: test_edge_uniform_write_error_vec3() ~= vec3(0.1, 0.1, 0.1)

mat4 test_edge_uniform_write_error_mat4() {
    // Reading uniform mat4 is allowed
    return transform;
}

// @unimplemented(jit.q32)
// run: test_edge_uniform_write_error_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

float test_edge_uniform_write_error_calculations() {
    // Complex calculations using uniform values
    float scaled_time = time * 2.0;
    vec2 offset_pos = position + vec2(0.5, 0.5);
    vec3 bright_color = color + vec3(0.2, 0.2, 0.2);

    return scaled_time + offset_pos.x + offset_pos.y + bright_color.x;
}

// Zeros + 0.5+0.5 + 0.2 from vec3(0.2,…) first component
// run: test_edge_uniform_write_error_calculations() ~= 1.2
