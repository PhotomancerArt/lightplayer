// test run

// ============================================================================
// Shared Global Declarations: Global variables with shared qualifier
// ============================================================================

shared float workgroup_counter;
shared int workgroup_id;
shared uint workgroup_size;
shared bool workgroup_flag;
shared vec2 workgroup_position;
shared vec3 workgroup_normal;
shared vec4 workgroup_color;
shared mat2 workgroup_transform;
shared float workgroup_data[64];

float test_declare_shared_float() {
    // Shared global float declaration
    // Note: shared variables are shared across workgroup
    workgroup_counter = 42.0;
    return workgroup_counter;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_float() ~= 42.0

int test_declare_shared_int() {
    // Shared global int declaration
    workgroup_id = 123;
    return workgroup_id;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_int() == 123

uint test_declare_shared_uint() {
    // Shared global uint declaration
    workgroup_size = 256u;
    return int(workgroup_size);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_uint() == 256

bool test_declare_shared_bool() {
    // Shared global bool declaration
    workgroup_flag = true;
    return workgroup_flag;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_bool() == true

vec2 test_declare_shared_vec2() {
    // Shared global vec2 declaration
    workgroup_position = vec2(10.0, 20.0);
    return workgroup_position;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_vec2() ~= vec2(10.0, 20.0)

vec3 test_declare_shared_vec3() {
    // Shared global vec3 declaration
    workgroup_normal = vec3(0.0, 1.0, 0.0);
    return workgroup_normal;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_vec3() ~= vec3(0.0, 1.0, 0.0)

vec4 test_declare_shared_vec4() {
    // Shared global vec4 declaration
    workgroup_color = vec4(1.0, 0.5, 0.0, 1.0);
    return workgroup_color;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_vec4() ~= vec4(1.0, 0.5, 0.0, 1.0)

mat2 test_declare_shared_mat2() {
    // Shared global mat2 declaration
    workgroup_transform = mat2(1.0, 0.0, 0.0, 1.0);
    return workgroup_transform;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0)

float test_declare_shared_array() {
    // Shared global array declaration
    workgroup_data[0] = 1.0;
    workgroup_data[1] = 2.0;
    workgroup_data[2] = 3.0;

    return workgroup_data[0] + workgroup_data[1] + workgroup_data[2];
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_declare_shared_array() ~= 6.0
