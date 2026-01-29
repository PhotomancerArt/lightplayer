// test run
// target riscv32.q32

// ============================================================================
// Edge No Storage Qualifier: Default behavior when no qualifier is specified
// ============================================================================

// Globals without storage qualifiers have default behavior
float default_float;
int default_int;
vec2 default_vec2;
vec3 default_vec3;
mat4 default_mat4;

float test_edge_no_storage_qualifier_float() {
    // Default global float - can read and write
    default_float = 42.0;
    return default_float;
}

// run: test_edge_no_storage_qualifier_float() ~= 42.0 [expect-fail]

int test_edge_no_storage_qualifier_int() {
    // Default global int - can read and write
    default_int = -123;
    return default_int;
}

// run: test_edge_no_storage_qualifier_int() == -123 [expect-fail]

vec2 test_edge_no_storage_qualifier_vec2() {
    // Default global vec2 - can read and write
    default_vec2 = vec2(1.0, 2.0);
    return default_vec2;
}

// run: test_edge_no_storage_qualifier_vec2() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_edge_no_storage_qualifier_vec3() {
    // Default global vec3 - can read and write
    default_vec3 = vec3(1.0, 2.0, 3.0);
    return default_vec3;
}

// run: test_edge_no_storage_qualifier_vec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

mat4 test_edge_no_storage_qualifier_mat4() {
    // Default global mat4 - can read and write
    default_mat4 = mat4(1.0);
    return default_mat4;
}

// run: test_edge_no_storage_qualifier_mat4() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0) [expect-fail]

float test_edge_no_storage_qualifier_modify() {
    // Modify default globals multiple times
    default_float = 10.0;
    default_float = default_float * 2.0;
    default_float = default_float + 5.0;

    return default_float;
}

// run: test_edge_no_storage_qualifier_modify() ~= 25.0 [expect-fail]

vec3 test_edge_no_storage_qualifier_vector_math() {
    // Vector math with default globals
    default_vec3 = vec3(1.0, 1.0, 1.0);
    default_vec3 = default_vec3 * 2.0;
    default_vec3 = default_vec3 + vec3(0.5, 0.5, 0.5);

    return default_vec3;
}

// run: test_edge_no_storage_qualifier_vector_math() ~= vec3(2.5, 2.5, 2.5) [expect-fail]

float test_edge_no_storage_qualifier_combined() {
    // Combined operations on default globals
    default_float = 5.0;
    default_int = 3;
    default_vec2 = vec2(2.0, 4.0);

    return default_float + float(default_int) + default_vec2.x + default_vec2.y;
}

// run: test_edge_no_storage_qualifier_combined() ~= 14.0 [expect-fail]
