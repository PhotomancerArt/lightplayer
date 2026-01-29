// test run
// target riscv32.q32

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

// run: test_type_vector_vec2() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_type_vector_vec3() {
    // Global vec3 variable
    global_vec3 = vec3(1.0, 2.0, 3.0);
    return global_vec3;
}

// run: test_type_vector_vec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

vec4 test_type_vector_vec4() {
    // Global vec4 variable
    global_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    return global_vec4;
}

// run: test_type_vector_vec4() ~= vec4(1.0, 2.0, 3.0, 4.0) [expect-fail]

ivec2 test_type_vector_ivec2() {
    // Global ivec2 variable
    global_ivec2 = ivec2(1, 2);
    return vec2(global_ivec2);
}

// run: test_type_vector_ivec2() ~= vec2(1.0, 2.0) [expect-fail]

ivec3 test_type_vector_ivec3() {
    // Global ivec3 variable
    global_ivec3 = ivec3(1, 2, 3);
    return vec3(global_ivec3);
}

// run: test_type_vector_ivec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

ivec4 test_type_vector_ivec4() {
    // Global ivec4 variable
    global_ivec4 = ivec4(1, 2, 3, 4);
    return vec4(global_ivec4);
}

// run: test_type_vector_ivec4() ~= vec4(1.0, 2.0, 3.0, 4.0) [expect-fail]

uvec2 test_type_vector_uvec2() {
    // Global uvec2 variable
    global_uvec2 = uvec2(1u, 2u);
    return vec2(global_uvec2);
}

// run: test_type_vector_uvec2() ~= vec2(1.0, 2.0) [expect-fail]

uvec3 test_type_vector_uvec3() {
    // Global uvec3 variable
    global_uvec3 = uvec3(1u, 2u, 3u);
    return vec3(global_uvec3);
}

// run: test_type_vector_uvec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

uvec4 test_type_vector_uvec4() {
    // Global uvec4 variable
    global_uvec4 = uvec4(1u, 2u, 3u, 4u);
    return vec4(global_uvec4);
}

// run: test_type_vector_uvec4() ~= vec4(1.0, 2.0, 3.0, 4.0) [expect-fail]

bvec2 test_type_vector_bvec2() {
    // Global bvec2 variable
    global_bvec2 = bvec2(true, false);
    return vec2(global_bvec2);
}

// run: test_type_vector_bvec2() ~= vec2(1.0, 0.0) [expect-fail]

bvec3 test_type_vector_bvec3() {
    // Global bvec3 variable
    global_bvec3 = bvec3(true, false, true);
    return vec3(global_bvec3);
}

// run: test_type_vector_bvec3() ~= vec3(1.0, 0.0, 1.0) [expect-fail]

bvec4 test_type_vector_bvec4() {
    // Global bvec4 variable
    global_bvec4 = bvec4(true, false, true, false);
    return vec4(global_bvec4);
}

// run: test_type_vector_bvec4() ~= vec4(1.0, 0.0, 1.0, 0.0) [expect-fail]

vec2 test_type_vector_operations() {
    // Vector operations on global vec2
    global_vec2 = vec2(1.0, 2.0);
    global_vec2 = global_vec2 * 2.0;
    global_vec2 = global_vec2 + vec2(1.0, 1.0);
    return global_vec2;
}

// run: test_type_vector_operations() ~= vec2(3.0, 5.0) [expect-fail]

vec3 test_type_vector_swizzle() {
    // Vector swizzling with global vec3
    global_vec3 = vec3(1.0, 2.0, 3.0);
    vec2 xy = global_vec3.xy;  // (1.0, 2.0)
    float z = global_vec3.z;   // 3.0
    return vec3(xy, z);
}

// run: test_type_vector_swizzle() ~= vec3(1.0, 2.0, 3.0) [expect-fail]
