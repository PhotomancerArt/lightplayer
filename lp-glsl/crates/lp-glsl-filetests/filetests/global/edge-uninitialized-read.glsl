// test run
// target riscv32.q32

// ============================================================================
// Edge Uninitialized Read: Reading uninitialized globals produces undefined behavior
// ============================================================================

float uninit_float;
int uninit_int;
uint uninit_uint;
bool uninit_bool;
vec2 uninit_vec2;
vec3 uninit_vec3;
vec4 uninit_vec4;
mat2 uninit_mat2;

float test_edge_uninitialized_read_float() {
    // Reading uninitialized float - undefined value
    return uninit_float + 1.0;
}

// run: test_edge_uninitialized_read_float() ~= 1.0 [expect-fail]

int test_edge_uninitialized_read_int() {
    // Reading uninitialized int - undefined value
    return uninit_int + 10;
}

// run: test_edge_uninitialized_read_int() == 10 [expect-fail]

uint test_edge_uninitialized_read_uint() {
    // Reading uninitialized uint - undefined value
    return int(uninit_uint + 5u);
}

// run: test_edge_uninitialized_read_uint() == 5 [expect-fail]

bool test_edge_uninitialized_read_bool() {
    // Reading uninitialized bool - undefined value
    return uninit_bool || true;
}

// run: test_edge_uninitialized_read_bool() == true [expect-fail]

vec2 test_edge_uninitialized_read_vec2() {
    // Reading uninitialized vec2 - undefined values
    return uninit_vec2 + vec2(1.0, 1.0);
}

// run: test_edge_uninitialized_read_vec2() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_edge_uninitialized_read_vec3() {
    // Reading uninitialized vec3 - undefined values
    return uninit_vec3 + vec3(1.0, 1.0, 1.0);
}

// run: test_edge_uninitialized_read_vec3() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

vec4 test_edge_uninitialized_read_vec4() {
    // Reading uninitialized vec4 - undefined values
    return uninit_vec4 + vec4(1.0, 1.0, 1.0, 1.0);
}

// run: test_edge_uninitialized_read_vec4() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

mat2 test_edge_uninitialized_read_mat2() {
    // Reading uninitialized mat2 - undefined values
    return uninit_mat2 + mat2(1.0);
}

// run: test_edge_uninitialized_read_mat2() ~= mat2(1.0, 1.0, 1.0, 1.0) [expect-fail]

void test_edge_uninitialized_assign_then_read() {
    // Assign values then read - should be defined
    uninit_float = 42.0;
    uninit_int = 123;
    uninit_uint = 456u;
    uninit_bool = true;
    uninit_vec2 = vec2(1.0, 2.0);
    uninit_vec3 = vec3(1.0, 2.0, 3.0);
    uninit_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    uninit_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
}

// run: test_edge_uninitialized_assign_then_read() == 0.0 [expect-fail]

float test_edge_uninitialized_after_assign() {
    // Read after assignment - should be defined
    test_edge_uninitialized_assign_then_read();
    return uninit_float + float(uninit_int);
}

// run: test_edge_uninitialized_after_assign() ~= 165.0 [expect-fail]
