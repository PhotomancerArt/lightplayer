// test run

// ============================================================================
// Vector Global Types: Global variables of vector types (vec2, vec3, vec4, etc.)
// ============================================================================

vec2 global_vec2;
vec3 global_vec3;
vec4 global_vec4;
ivec2 global_ivec2;
ivec3 global_ivec3;
ivec4 global_ivec4;
uvec2 global_uvec2;
uvec3 global_uvec3;
uvec4 global_uvec4;
bvec2 global_bvec2;
bvec3 global_bvec3;
bvec4 global_bvec4;

vec2 test_type_vector_vec2() {
    // Global vec2 variable
    global_vec2 = vec2(1.0, 2.0);
    return global_vec2;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_vec2() ~= vec2(1.0, 2.0)

vec3 test_type_vector_vec3() {
    // Global vec3 variable
    global_vec3 = vec3(1.0, 2.0, 3.0);
    return global_vec3;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_vec3() ~= vec3(1.0, 2.0, 3.0)

vec4 test_type_vector_vec4() {
    // Global vec4 variable
    global_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    return global_vec4;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_vec4() ~= vec4(1.0, 2.0, 3.0, 4.0)

ivec2 test_type_vector_ivec2() {
    // Global ivec2 variable
    global_ivec2 = ivec2(1, 2);
    return vec2(global_ivec2);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_ivec2() ~= vec2(1.0, 2.0)

ivec3 test_type_vector_ivec3() {
    // Global ivec3 variable
    global_ivec3 = ivec3(1, 2, 3);
    return vec3(global_ivec3);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_ivec3() ~= vec3(1.0, 2.0, 3.0)

ivec4 test_type_vector_ivec4() {
    // Global ivec4 variable
    global_ivec4 = ivec4(1, 2, 3, 4);
    return vec4(global_ivec4);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_ivec4() ~= vec4(1.0, 2.0, 3.0, 4.0)

uvec2 test_type_vector_uvec2() {
    // Global uvec2 variable
    global_uvec2 = uvec2(1u, 2u);
    return vec2(global_uvec2);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_uvec2() ~= vec2(1.0, 2.0)

uvec3 test_type_vector_uvec3() {
    // Global uvec3 variable
    global_uvec3 = uvec3(1u, 2u, 3u);
    return vec3(global_uvec3);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_uvec3() ~= vec3(1.0, 2.0, 3.0)

uvec4 test_type_vector_uvec4() {
    // Global uvec4 variable
    global_uvec4 = uvec4(1u, 2u, 3u, 4u);
    return vec4(global_uvec4);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_uvec4() ~= vec4(1.0, 2.0, 3.0, 4.0)

bvec2 test_type_vector_bvec2() {
    // Global bvec2 variable
    global_bvec2 = bvec2(true, false);
    return vec2(global_bvec2);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_bvec2() ~= vec2(1.0, 0.0)

bvec3 test_type_vector_bvec3() {
    // Global bvec3 variable
    global_bvec3 = bvec3(true, false, true);
    return vec3(global_bvec3);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_bvec3() ~= vec3(1.0, 0.0, 1.0)

bvec4 test_type_vector_bvec4() {
    // Global bvec4 variable
    global_bvec4 = bvec4(true, false, true, false);
    return vec4(global_bvec4);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_bvec4() ~= vec4(1.0, 0.0, 1.0, 0.0)

vec2 test_type_vector_operations() {
    // Vector operations on global vec2
    global_vec2 = vec2(1.0, 2.0);
    global_vec2 = global_vec2 * 2.0;
    global_vec2 = global_vec2 + vec2(1.0, 1.0);
    return global_vec2;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_operations() ~= vec2(3.0, 5.0)

vec3 test_type_vector_swizzle() {
    // Vector swizzling with global vec3
    global_vec3 = vec3(1.0, 2.0, 3.0);
    vec2 xy = global_vec3.xy;  // (1.0, 2.0)
    float z = global_vec3.z;   // 3.0
    return vec3(xy, z);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_type_vector_swizzle() ~= vec3(1.0, 2.0, 3.0)
