// test run
// target riscv32.q32

// ============================================================================
// Input Global Declarations: Global variables with in qualifier
// ============================================================================

in float time;
in int vertex_id;
in uint instance_id;
in bool selected;
in vec2 tex_coord;
in vec3 position;
in vec4 color;

float test_declare_in_float() {
    // Input global float declaration
    // Note: in variables are read-only
    return time + 1.0;
}

// run: test_declare_in_float() ~= 1.0 [expect-fail]

int test_declare_in_int() {
    // Input global int declaration
    return vertex_id * 2;
}

// run: test_declare_in_int() == 0 [expect-fail]

uint test_declare_in_uint() {
    // Input global uint declaration
    return int(instance_id + 1u);
}

// run: test_declare_in_uint() == 1 [expect-fail]

bool test_declare_in_bool() {
    // Input global bool declaration
    return selected;
}

// run: test_declare_in_bool() == false [expect-fail]

vec2 test_declare_in_vec2() {
    // Input global vec2 declaration
    return tex_coord + vec2(0.5, 0.5);
}

// run: test_declare_in_vec2() ~= vec2(0.5, 0.5) [expect-fail]

vec3 test_declare_in_vec3() {
    // Input global vec3 declaration
    return position * 2.0;
}

// run: test_declare_in_vec3() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

vec4 test_declare_in_vec4() {
    // Input global vec4 declaration
    return color;
}

// run: test_declare_in_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_declare_in_calculate() {
    // Input globals used in calculations
    float scaled_time = time * 2.0;
    vec2 adjusted_tex = tex_coord + vec2(0.1, 0.1);
    return scaled_time + adjusted_tex.x + adjusted_tex.y;
}

// run: test_declare_in_calculate() ~= 0.2 [expect-fail]
