// test run
// target riscv32.q32

// ============================================================================
// Variable Indexing: Valid indices
// ============================================================================

bool test_bvec2_variable_index_0() {
    bvec2 a = bvec2(true, false);
    int i = 0;
    return a[i];
}

// run: test_bvec2_variable_index_0() == true

bool test_bvec2_variable_index_1() {
    bvec2 a = bvec2(true, false);
    int i = 1;
    return a[i];
}

// run: test_bvec2_variable_index_1() == false

bool test_bvec2_variable_index_computed() {
    bvec2 a = bvec2(false, true);
    int i = int(any(bvec2(true, false))); // i = 1
    return a[i];
}

// run: test_bvec2_variable_index_computed() == true

bool test_bvec2_variable_index_expression() {
    bvec2 a = bvec2(true, false);
    return a[1 - 1]; // Should be equivalent to a[0]
}

// run: test_bvec2_variable_index_expression() == true

bool test_bvec3_variable_index() {
    bvec3 a = bvec3(true, false, true);
    int i = 1;
    return a[i];
}

// run: test_bvec3_variable_index() == false

bool test_bvec4_variable_index() {
    bvec4 a = bvec4(true, false, true, false);
    int i = 2;
    return a[i];
}

// run: test_bvec4_variable_index() == true

int test_ivec2_variable_index() {
    ivec2 a = ivec2(10, 20);
    int i = 1;
    return a[i];
}

// run: test_ivec2_variable_index() == 20

int test_ivec3_variable_index() {
    ivec3 a = ivec3(1, 2, 3);
    int i = 0;
    return a[i];
}

// run: test_ivec3_variable_index() == 1

float test_vec2_variable_index() {
    vec2 a = vec2(1.5, 2.5);
    int i = 1;
    return a[i];
}

// run: test_vec2_variable_index() ~= 2.5

float test_vec3_variable_index() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    int i = 2;
    return a[i];
}

// run: test_vec3_variable_index() ~= 3.0

float test_vec4_variable_index() {
    vec4 a = vec4(1.0, 2.0, 3.0, 4.0);
    int i = 3;
    return a[i];
}

// run: test_vec4_variable_index() ~= 4.0

