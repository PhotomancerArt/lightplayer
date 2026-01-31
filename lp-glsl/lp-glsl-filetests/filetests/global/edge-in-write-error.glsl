// test run
// target riscv32.q32

// ============================================================================
// Edge In Write Error: Writing to input globals is a compile error
// ============================================================================

in float vertex_time;
in int vertex_id;
in vec2 tex_coord;
in vec3 vertex_position;
in vec4 vertex_color;

// These would be compile errors (input variables are read-only):
// vertex_time = 5.0;                    // Error: cannot assign to input
// vertex_id = 42;                       // Error: cannot assign to input
// tex_coord = vec2(0.5, 0.5);           // Error: cannot assign to input
// vertex_position = vec3(1.0, 1.0, 1.0); // Error: cannot assign to input
// vertex_color = vec4(1.0, 1.0, 1.0, 1.0); // Error: cannot assign to input

// However, reading from input variables is allowed
float test_edge_in_write_error_read() {
    // Reading from input is allowed
    return vertex_time + 1.0;
}

// run: test_edge_in_write_error_read() ~= 1.0 [expect-fail]

int test_edge_in_write_error_int() {
    // Reading input int is allowed
    return vertex_id + 10;
}

// run: test_edge_in_write_error_int() == 10 [expect-fail]

vec2 test_edge_in_write_error_vec2() {
    // Reading input vec2 is allowed
    return tex_coord * 2.0;
}

// run: test_edge_in_write_error_vec2() ~= vec2(0.0, 0.0) [expect-fail]

vec3 test_edge_in_write_error_vec3() {
    // Reading input vec3 is allowed
    return vertex_position + vec3(0.0, 1.0, 0.0);
}

// run: test_edge_in_write_error_vec3() ~= vec3(0.0, 1.0, 0.0) [expect-fail]

vec4 test_edge_in_write_error_vec4() {
    // Reading input vec4 is allowed
    return vertex_color;
}

// run: test_edge_in_write_error_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_edge_in_write_error_calculations() {
    // Complex calculations using input values
    float scaled_time = vertex_time * 2.0;
    vec2 adjusted_texcoord = tex_coord + vec2(0.1, 0.1);
    vec3 elevated_position = vertex_position + vec3(0.0, 5.0, 0.0);

    return scaled_time + adjusted_texcoord.x + adjusted_texcoord.y +
           elevated_position.x + elevated_position.y + elevated_position.z;
}

// run: test_edge_in_write_error_calculations() ~= 7.2 [expect-fail]

vec4 test_edge_in_write_error_vertex_processing() {
    // Vertex processing using input values
    vec4 processed_color = vertex_color;

    if (vertex_id > 0) {
        processed_color = processed_color * 0.8;
    }

    processed_color.a = float(vertex_id) / 100.0;

    return processed_color;
}

// run: test_edge_in_write_error_vertex_processing() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]
