// test run
// @unimplemented(backend=wasm)

// ============================================================================
// Simple Global Initialization: Global variables initialized without qualifier
// ============================================================================

float global_float_init = 42.0;
int global_int_init = -123;
uint global_uint_init = 987u;
bool global_bool_init = true;
vec2 global_vec2_init = vec2(1.0, 2.0);
vec3 global_vec3_init = vec3(1.0, 2.0, 3.0);
vec4 global_vec4_init = vec4(1.0, 2.0, 3.0, 4.0);
mat2 global_mat2_init = mat2(1.0, 2.0, 3.0, 4.0);

float test_initialize_simple_float() {
    // Simple global float initialization
    return global_float_init * 2.0;
}

// @unimplemented()
// run: test_initialize_simple_float() ~= 84.0

int test_initialize_simple_int() {
    // Simple global int initialization
    return global_int_init + 100;
}

// @unimplemented()
// run: test_initialize_simple_int() == -23

uint test_initialize_simple_uint() {
    // Simple global uint initialization
    return int(global_uint_init / 3u);
}

// @unimplemented()
// run: test_initialize_simple_uint() == 329

bool test_initialize_simple_bool() {
    // Simple global bool initialization
    return global_bool_init;
}

// @unimplemented()
// run: test_initialize_simple_bool() == true

vec2 test_initialize_simple_vec2() {
    // Simple global vec2 initialization
    return global_vec2_init * 2.0;
}

// @unimplemented()
// run: test_initialize_simple_vec2() ~= vec2(2.0, 4.0)

vec3 test_initialize_simple_vec3() {
    // Simple global vec3 initialization
    return global_vec3_init + vec3(1.0, 1.0, 1.0);
}

// @unimplemented()
// run: test_initialize_simple_vec3() ~= vec3(2.0, 3.0, 4.0)

vec4 test_initialize_simple_vec4() {
    // Simple global vec4 initialization
    return global_vec4_init;
}

// @unimplemented()
// run: test_initialize_simple_vec4() ~= vec4(1.0, 2.0, 3.0, 4.0)

mat2 test_initialize_simple_mat2() {
    // Simple global mat2 initialization
    return global_mat2_init * 2.0;
}

// @unimplemented()
// run: test_initialize_simple_mat2() ~= mat2(2.0, 4.0, 6.0, 8.0)

float test_initialize_simple_modify() {
    // Simple global initialization then modification
    float temp = global_float_init;
    global_float_init = global_float_init + 10.0;
    return global_float_init;
}

// @unimplemented()
// run: test_initialize_simple_modify() ~= 52.0
