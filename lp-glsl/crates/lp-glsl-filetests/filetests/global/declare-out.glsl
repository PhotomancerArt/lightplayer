// test run
// target riscv32.q32

// ============================================================================
// Output Global Declarations: Global variables with out qualifier
// ============================================================================

out float depth;
out int stencil_value;
out uint object_id;
out bool visible;
out vec2 screen_pos;
out vec3 normal;
out vec4 frag_color;

void test_declare_out_float() {
    // Output global float declaration
    // Note: out variables are write-only
    depth = 0.5;
}

// run: test_declare_out_float() == 0.0 [expect-fail]

void test_declare_out_int() {
    // Output global int declaration
    stencil_value = 128;
}

// run: test_declare_out_int() == 0.0 [expect-fail]

void test_declare_out_uint() {
    // Output global uint declaration
    object_id = 42u;
}

// run: test_declare_out_uint() == 0.0 [expect-fail]

void test_declare_out_bool() {
    // Output global bool declaration
    visible = true;
}

// run: test_declare_out_bool() == 0.0 [expect-fail]

void test_declare_out_vec2() {
    // Output global vec2 declaration
    screen_pos = vec2(0.5, 0.5);
}

// run: test_declare_out_vec2() == 0.0 [expect-fail]

void test_declare_out_vec3() {
    // Output global vec3 declaration
    normal = vec3(0.0, 1.0, 0.0);
}

// run: test_declare_out_vec3() == 0.0 [expect-fail]

void test_declare_out_vec4() {
    // Output global vec4 declaration
    frag_color = vec4(1.0, 0.0, 0.0, 1.0);
}

// run: test_declare_out_vec4() == 0.0 [expect-fail]

void test_declare_out_calculate() {
    // Output globals with calculations
    depth = 0.25;
    screen_pos = vec2(0.3, 0.7);
    frag_color = vec4(0.5, 0.5, 0.5, 1.0);
    visible = true;
}

// run: test_declare_out_calculate() == 0.0 [expect-fail]
