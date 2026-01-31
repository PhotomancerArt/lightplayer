// test run
// target riscv32.q32

// ============================================================================
// Ternary operator with different types
// Spec: Second and third expressions can be any type (except opaque types)
//       including vectors, matrices, structures, arrays
// ============================================================================

// Vector types
int test_ternary_vec2() {
    bool b = true;
    vec2 v1 = vec2(1.0, 2.0);
    vec2 v2 = vec2(3.0, 4.0);
    vec2 result = b ? v1 : v2;
    return int(result.x + result.y);
}

// run: test_ternary_vec2() == 3 [expect-fail]

int test_ternary_vec3() {
    bool b = false;
    vec3 v1 = vec3(1.0, 2.0, 3.0);
    vec3 v2 = vec3(4.0, 5.0, 6.0);
    vec3 result = b ? v1 : v2;
    return int(result.x + result.y + result.z);
}

// run: test_ternary_vec3() == 15 [expect-fail]

int test_ternary_vec4() {
    bool b = true;
    vec4 v1 = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 v2 = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 result = b ? v1 : v2;
    return int(result.x + result.y);
}

// run: test_ternary_vec4() == 3 [expect-fail]

int test_ternary_ivec2() {
    bool b = false;
    ivec2 v1 = ivec2(10, 20);
    ivec2 v2 = ivec2(30, 40);
    ivec2 result = b ? v1 : v2;
    return result.x + result.y;
}

// run: test_ternary_ivec2() == 70 [expect-fail]

int test_ternary_bvec2() {
    bool b = true;
    bvec2 v1 = bvec2(true, false);
    bvec2 v2 = bvec2(false, true);
    bvec2 result = b ? v1 : v2;
    return (result.x ? 1 : 0) + (result.y ? 1 : 0);
}

// run: test_ternary_bvec2() == 1 [expect-fail]

// Matrix types
int test_ternary_mat2() {
    bool b = true;
    mat2 m1 = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 m2 = mat2(5.0, 6.0, 7.0, 8.0);
    mat2 result = b ? m1 : m2;
    return int(result[0][0] + result[1][1]);
}

// run: test_ternary_mat2() == 5 [expect-fail]

int test_ternary_mat3() {
    bool b = false;
    mat3 m1 = mat3(1.0);
    mat3 m2 = mat3(2.0);
    mat3 result = b ? m1 : m2;
    return int(result[0][0] * 10.0);
}

// run: test_ternary_mat3() == 20 [expect-fail]

// Structure types
struct Point {
    float x;
    float y;
};

int test_ternary_struct() {
    bool b = true;
    Point p1 = Point(1.0, 2.0);
    Point p2 = Point(3.0, 4.0);
    Point result = b ? p1 : p2;
    return int(result.x + result.y);
}

// run: test_ternary_struct() == 3 [expect-fail]

struct Color {
    float r;
    float g;
    float b;
};

int test_ternary_struct_complex() {
    bool b = false;
    Color c1 = Color(0.1, 0.2, 0.3);
    Color c2 = Color(0.4, 0.5, 0.6);
    Color result = b ? c1 : c2;
    return int((result.r + result.g + result.b) * 10.0);
}

// run: test_ternary_struct_complex() == 15 [expect-fail]

// Array types (if supported)
int test_ternary_array_element() {
    bool b = true;
    int arr1[3] = int[3](1, 2, 3);
    int arr2[3] = int[3](4, 5, 6);
    // Can't assign arrays directly, but can use elements
    int result = b ? arr1[0] : arr2[0];
    return result;
}

// run: test_ternary_array_element() == 1 [expect-fail]

// Mixed vector component access
int test_ternary_vec_component() {
    bool b = false;
    vec3 v1 = vec3(1.0, 2.0, 3.0);
    vec3 v2 = vec3(4.0, 5.0, 6.0);
    float result = b ? v1.x : v2.y;
    return int(result);
}

// run: test_ternary_vec_component() == 5 [expect-fail]

// Nested with different types
int test_ternary_nested_types() {
    bool b1 = true;
    bool b2 = false;
    vec2 v1 = vec2(1.0, 2.0);
    vec2 v2 = vec2(3.0, 4.0);
    vec2 v3 = vec2(5.0, 6.0);
    vec2 result = b1 ? (b2 ? v1 : v2) : v3;
    return int(result.x + result.y);
}

// run: test_ternary_nested_types() == 7 [expect-fail]





