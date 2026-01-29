// test run
// target riscv32.q32

// ============================================================================
// Global Variable Write Access: Writing to global variables
// ============================================================================

float global_float;
int global_int;
uint global_uint;
bool global_bool;
vec2 global_vec2;
vec3 global_vec3;
vec4 global_vec4;
mat2 global_mat2;

out float out_depth;
buffer DataBlock { float buffer_data[4]; };
shared float shared_counter;

void test_access_write_float() {
    // Writing to global float
    global_float = 42.0;
}

// run: test_access_write_float() == 0.0 [expect-fail]

void test_access_write_int() {
    // Writing to global int
    global_int = -123;
}

// run: test_access_write_int() == 0.0 [expect-fail]

void test_access_write_uint() {
    // Writing to global uint
    global_uint = 987u;
}

// run: test_access_write_uint() == 0.0 [expect-fail]

void test_access_write_bool() {
    // Writing to global bool
    global_bool = true;
}

// run: test_access_write_bool() == 0.0 [expect-fail]

void test_access_write_vec2() {
    // Writing to global vec2
    global_vec2 = vec2(1.0, 2.0);
}

// run: test_access_write_vec2() == 0.0 [expect-fail]

void test_access_write_vec3() {
    // Writing to global vec3
    global_vec3 = vec3(1.0, 2.0, 3.0);
}

// run: test_access_write_vec3() == 0.0 [expect-fail]

void test_access_write_vec4() {
    // Writing to global vec4
    global_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
}

// run: test_access_write_vec4() == 0.0 [expect-fail]

void test_access_write_mat2() {
    // Writing to global mat2
    global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
}

// run: test_access_write_mat2() == 0.0 [expect-fail]

void test_access_write_out() {
    // Writing to out global
    out_depth = 0.5;
}

// run: test_access_write_out() == 0.0 [expect-fail]

void test_access_write_buffer() {
    // Writing to buffer global
    buffer_data[0] = 5.0;
    buffer_data[1] = 10.0;
    buffer_data[2] = 15.0;
    buffer_data[3] = 20.0;
}

// run: test_access_write_buffer() == 0.0 [expect-fail]

void test_access_write_shared() {
    // Writing to shared global
    shared_counter = 42.0;
}

// run: test_access_write_shared() == 0.0 [expect-fail]

float test_access_write_read() {
    // Write then read global
    global_float = 100.0;
    global_int = 50;
    global_vec2 = vec2(3.0, 4.0);

    return global_float + float(global_int) + global_vec2.x + global_vec2.y;
}

// run: test_access_write_read() ~= 157.0 [expect-fail]
