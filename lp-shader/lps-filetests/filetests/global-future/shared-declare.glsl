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

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// wasm.f32: shader does not compile on any target (frontend gap) — same cause
// as the @unsupported entries above, not an f32-specific failure.
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_float() ~= 42.0

int test_declare_shared_int() {
    // Shared global int declaration
    workgroup_id = 123;
    return workgroup_id;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_int() == 123

uint test_declare_shared_uint() {
    // Shared global uint declaration
    workgroup_size = 256u;
    return int(workgroup_size);
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_uint() == 256

bool test_declare_shared_bool() {
    // Shared global bool declaration
    workgroup_flag = true;
    return workgroup_flag;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_bool() == true

vec2 test_declare_shared_vec2() {
    // Shared global vec2 declaration
    workgroup_position = vec2(10.0, 20.0);
    return workgroup_position;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_vec2() ~= vec2(10.0, 20.0)

vec3 test_declare_shared_vec3() {
    // Shared global vec3 declaration
    workgroup_normal = vec3(0.0, 1.0, 0.0);
    return workgroup_normal;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_vec3() ~= vec3(0.0, 1.0, 0.0)

vec4 test_declare_shared_vec4() {
    // Shared global vec4 declaration
    workgroup_color = vec4(1.0, 0.5, 0.0, 1.0);
    return workgroup_color;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_vec4() ~= vec4(1.0, 0.5, 0.0, 1.0)

mat2 test_declare_shared_mat2() {
    // Shared global mat2 declaration
    workgroup_transform = mat2(1.0, 0.0, 0.0, 1.0);
    return workgroup_transform;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_mat2() ~= mat2(1.0, 0.0, 0.0, 1.0)

float test_declare_shared_array() {
    // Shared global array declaration
    workgroup_data[0] = 1.0;
    workgroup_data[1] = 2.0;
    workgroup_data[2] = 3.0;

    return workgroup_data[0] + workgroup_data[1] + workgroup_data[2];
}

// wgpu.f32: file does not compile through naga glsl-in (mirrors the interp.f32 frontend gap)
// @unimplemented(frontend!=lp, backend!=wgpu)
// @unsupported(rv32lpn.q32)
// @unsupported(xtlpn.q32)
// @unsupported(wgpu.f32)
// @unsupported(wasm.f32)
// @unsupported(rv32lpn.f32)
// @unsupported(xtlpn.f32)
// run: test_declare_shared_array() ~= 6.0
