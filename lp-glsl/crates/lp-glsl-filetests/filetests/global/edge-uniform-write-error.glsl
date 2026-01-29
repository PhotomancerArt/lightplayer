// test run
// target riscv32.q32

// ============================================================================
// Edge Uniform Write Error: Writing to uniform globals may be a compile error
// ============================================================================

uniform float time;
uniform int count;
uniform vec2 position;
uniform vec3 color;
uniform mat4 transform;

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

// run: test_edge_uniform_write_error_read() ~= 1.0 [expect-fail]

int test_edge_uniform_write_error_int() {
    // Reading uniform int is allowed
    return count + 5;
}

// run: test_edge_uniform_write_error_int() == 5 [expect-fail]

vec2 test_edge_uniform_write_error_vec2() {
    // Reading uniform vec2 is allowed
    return position * 2.0;
}

// run: test_edge_uniform_write_error_vec2() ~= vec2(0.0, 0.0) [expect-fail]

vec3 test_edge_uniform_write_error_vec3() {
    // Reading uniform vec3 is allowed
    return color + vec3(0.1, 0.1, 0.1);
}

// run: test_edge_uniform_write_error_vec3() ~= vec3(0.1, 0.1, 0.1) [expect-fail]

mat4 test_edge_uniform_write_error_mat4() {
    // Reading uniform mat4 is allowed
    return transform;
}

// run: test_edge_uniform_write_error_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_edge_uniform_write_error_calculations() {
    // Complex calculations using uniform values
    float scaled_time = time * 2.0;
    vec2 offset_pos = position + vec2(0.5, 0.5);
    vec3 bright_color = color + vec3(0.2, 0.2, 0.2);

    return scaled_time + offset_pos.x + offset_pos.y + bright_color.x;
}

// run: test_edge_uniform_write_error_calculations() ~= 2.7 [expect-fail]
