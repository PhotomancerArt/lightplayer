// test run
// target riscv32.q32

// ============================================================================
// Explicitly-sized Array Declarations
// ============================================================================

float test_declare_float_array() {
    float arr[5]; // array of 5 floats
    return 1.0; // Declaration test - no runtime behavior
}

// run: test_declare_float_array() == 1.0

int test_declare_int_array() {
    int arr[3]; // array of 3 ints
    return 1; // Declaration test
}

// run: test_declare_int_array() == 1

uint test_declare_uint_array() {
    uint arr[4]; // array of 4 uints
    return 1u; // Declaration test
}

// run: test_declare_uint_array() == 1u

bool test_declare_bool_array() {
    bool arr[6]; // array of 6 bools
    return true; // Declaration test
}

// run: test_declare_bool_array() == true

vec2 test_declare_vec2_array() {
    vec2 arr[2]; // array of 2 vec2s
    return vec2(1.0, 1.0); // Declaration test
}

// run: test_declare_vec2_array() ~= vec2(1.0, 1.0)

vec3 test_declare_vec3_array() {
    vec3 arr[3]; // array of 3 vec3s
    return vec3(1.0, 1.0, 1.0); // Declaration test
}

// run: test_declare_vec3_array() ~= vec3(1.0, 1.0, 1.0)

vec4 test_declare_vec4_array() {
    vec4 arr[2]; // array of 2 vec4s
    return vec4(1.0, 1.0, 1.0, 1.0); // Declaration test
}

// run: test_declare_vec4_array() ~= vec4(1.0, 1.0, 1.0, 1.0)

ivec2 test_declare_ivec2_array() {
    ivec2 arr[4]; // array of 4 ivec2s
    return ivec2(1, 1); // Declaration test
}

// run: test_declare_ivec2_array() == ivec2(1, 1)

uvec3 test_declare_uvec3_array() {
    uvec3 arr[2]; // array of 2 uvec3s
    return uvec3(1u, 1u, 1u); // Declaration test
}

// run: test_declare_uvec3_array() == uvec3(1u, 1u, 1u)

bvec4 test_declare_bvec4_array() {
    bvec4 arr[3]; // array of 3 bvec4s
    return bvec4(true, true, true, true); // Declaration test
}

// run: test_declare_bvec4_array() == bvec4(true, true, true, true)
