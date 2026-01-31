// test run
// target riscv32.q32

// ============================================================================
// Edge Out Read Error: Reading from output globals may be restricted
// ============================================================================

out float fragment_depth;
out int stencil_value;
out vec2 screen_position;
out vec3 normal_vector;
out vec4 fragment_color;

// Reading from output variables may be allowed or restricted depending on shader stage
// In some stages, outputs are write-only; in others, they may be readable

void test_edge_out_read_error_write() {
    // Writing to outputs is always allowed
    fragment_depth = 0.5;
    stencil_value = 128;
    screen_position = vec2(0.5, 0.5);
    normal_vector = vec3(0.0, 1.0, 0.0);
    fragment_color = vec4(1.0, 0.0, 0.0, 1.0);
}

// run: test_edge_out_read_error_write() == 0.0 [expect-fail]

// These reads may or may not be allowed depending on GLSL version and shader stage:
// float read_depth = fragment_depth;        // May be error in some stages
// int read_stencil = stencil_value;         // May be error in some stages
// vec2 read_screen = screen_position;       // May be error in some stages
// vec3 read_normal = normal_vector;         // May be error in some stages
// vec4 read_color = fragment_color;         // May be error in some stages

float test_edge_out_read_error_indirect() {
    // Test indirect access through writing
    fragment_depth = 0.25;
    screen_position = vec2(0.3, 0.7);
    fragment_color = vec4(0.5, 0.5, 0.5, 1.0);

    // We can't directly read back, but we can verify through calculations
    return 0.0;  // Placeholder
}

// run: test_edge_out_read_error_indirect() ~= 0.0 [expect-fail]

void test_edge_out_read_error_multiple_writes() {
    // Multiple writes to same output
    fragment_color = vec4(1.0, 0.0, 0.0, 1.0);
    fragment_color = vec4(0.0, 1.0, 0.0, 1.0);  // Overwrites previous
    fragment_color = vec4(0.0, 0.0, 1.0, 1.0);  // Final value
}

// run: test_edge_out_read_error_multiple_writes() == 0.0 [expect-fail]

void test_edge_out_read_error_fragment_output() {
    // Typical fragment shader output pattern
    vec3 base_color = vec3(0.8, 0.6, 0.4);
    float alpha = 1.0;

    fragment_color = vec4(base_color, alpha);
    fragment_depth = 0.0;
}

// run: test_edge_out_read_error_fragment_output() == 0.0 [expect-fail]
