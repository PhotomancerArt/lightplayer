// test run
// target riscv32.q32

// ============================================================================
// Simple Global Declarations: Global variables without storage qualifiers
// ============================================================================

float global_float;
int global_int;
uint global_uint;
bool global_bool;

float test_declare_simple_float() {
    // Simple global float declaration
    global_float = 42.0;
    return global_float;
}

// run: test_declare_simple_float() ~= 42.0 [expect-fail]

int test_declare_simple_int() {
    // Simple global int declaration
    global_int = -123;
    return global_int;
}

// run: test_declare_simple_int() == -123 [expect-fail]

uint test_declare_simple_uint() {
    // Simple global uint declaration
    global_uint = 987;
    return int(global_uint);
}

// run: test_declare_simple_uint() == 987 [expect-fail]

bool test_declare_simple_bool() {
    // Simple global bool declaration
    global_bool = true;
    return global_bool;
}

// run: test_declare_simple_bool() == true [expect-fail]

vec2 test_declare_simple_vec2() {
    // Simple global vec2 declaration
    vec2 global_vec2;
    global_vec2 = vec2(1.0, 2.0);
    return global_vec2;
}

// run: test_declare_simple_vec2() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_declare_simple_vec3() {
    // Simple global vec3 declaration
    vec3 global_vec3;
    global_vec3 = vec3(1.0, 2.0, 3.0);
    return global_vec3;
}

// run: test_declare_simple_vec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

vec4 test_declare_simple_vec4() {
    // Simple global vec4 declaration
    vec4 global_vec4;
    global_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    return global_vec4;
}

// run: test_declare_simple_vec4() ~= vec4(1.0, 2.0, 3.0, 4.0) [expect-fail]

mat2 test_declare_simple_mat2() {
    // Simple global mat2 declaration
    mat2 global_mat2;
    global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
    return global_mat2;
}

// run: test_declare_simple_mat2() ~= mat2(1.0, 2.0, 3.0, 4.0) [expect-fail]

float test_declare_simple_multiple() {
    // Multiple simple global declarations
    float a, b, c;

    a = 10.0;
    b = 20.0;
    c = 30.0;

    return a + b + c;
}

// run: test_declare_simple_multiple() ~= 60.0 [expect-fail]
