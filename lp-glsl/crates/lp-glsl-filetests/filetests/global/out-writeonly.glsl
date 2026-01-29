// test run
// target riscv32.q32

// ============================================================================
// Out Write-Only: Output global variables are write-only (may be read-write depending on stage)
// ============================================================================

out float fragment_depth;
out int stencil_value;
out uint object_id;
out bool visible;
out vec2 screen_position;
out vec3 normal_vector;
out vec4 fragment_color;

void test_out_writeonly_float() {
    // Output float is write-only
    fragment_depth = 0.5;
}

// run: test_out_writeonly_float() == 0.0 [expect-fail]

void test_out_writeonly_int() {
    // Output int is write-only
    stencil_value = 128;
}

// run: test_out_writeonly_int() == 0.0 [expect-fail]

void test_out_writeonly_uint() {
    // Output uint is write-only
    object_id = 42u;
}

// run: test_out_writeonly_uint() == 0.0 [expect-fail]

void test_out_writeonly_bool() {
    // Output bool is write-only
    visible = true;
}

// run: test_out_writeonly_bool() == 0.0 [expect-fail]

void test_out_writeonly_vec2() {
    // Output vec2 is write-only
    screen_position = vec2(0.5, 0.5);
}

// run: test_out_writeonly_vec2() == 0.0 [expect-fail]

void test_out_writeonly_vec3() {
    // Output vec3 is write-only
    normal_vector = vec3(0.0, 1.0, 0.0);
}

// run: test_out_writeonly_vec3() == 0.0 [expect-fail]

void test_out_writeonly_vec4() {
    // Output vec4 is write-only
    fragment_color = vec4(1.0, 0.0, 0.0, 1.0);
}

// run: test_out_writeonly_vec4() == 0.0 [expect-fail]

void test_out_writeonly_calculations() {
    // Output variables with calculations
    fragment_depth = 0.25;
    screen_position = vec2(0.3, 0.7);
    fragment_color = vec4(0.5, 0.5, 0.5, 1.0);
    visible = true;
    stencil_value = 255;
    object_id = 100u;
}

// run: test_out_writeonly_calculations() == 0.0 [expect-fail]

void test_out_writeonly_fragment_output() {
    // Fragment shader output calculations
    vec3 base_color = vec3(0.8, 0.6, 0.4);
    float alpha = 1.0;

    fragment_color = vec4(base_color, alpha);
    fragment_depth = 0.0;  // Write to depth
    visible = true;
}

// run: test_out_writeonly_fragment_output() == 0.0 [expect-fail]
