// test run
// target riscv32.q32

// ============================================================================
// Default Parameter Qualifier: 'in' is the default
// ============================================================================

float add_explicit(in float a, in float b) {
    return a + b;
}

float test_param_default_explicit_in() {
    // Explicit 'in' qualifier
    return add_explicit(2.0, 3.0);
}

// run: test_param_default_explicit_in() ~= 5.0 [expect-fail]

float add_implicit(float a, float b) {
    return a + b;
}

float test_param_default_implicit_in() {
    // Implicit 'in' qualifier (default)
    return add_implicit(2.0, 3.0);
}

// run: test_param_default_implicit_in() ~= 5.0 [expect-fail]

float process(in float a, float b, in float c) {
    return a + b + c;
}

float test_param_default_mixed() {
    // Mix of explicit and implicit in qualifiers
    return process(1.0, 2.0, 3.0);
}

// run: test_param_default_mixed() ~= 6.0 [expect-fail]

vec2 combine_vectors(vec2 a, vec2 b) {
    return a + b;
}

float test_param_default_vector() {
    // Default qualifier with vectors
    return length(combine_vectors(vec2(1.0, 2.0), vec2(3.0, 4.0)));
}

// run: test_param_default_vector() ~= 10.0 [expect-fail]

int multiply(int x, int y) {
    return x * y;
}

int test_param_default_int() {
    // Default qualifier with integers
    return multiply(6, 7);
}

// run: test_param_default_int() == 42 [expect-fail]

bool logical_and(bool a, bool b) {
    return a && b;
}

bool test_param_default_bool() {
    // Default qualifier with booleans
    return logical_and(true, true);
}

// run: test_param_default_bool() == true [expect-fail]

float modify_local(float x) {
    x = x + 10.0; // Modifies local copy only
    return x;
}

float test_param_default_modification() {
    // Parameters can be modified inside function (only affects local copy)
    float original = 5.0;
    float result = modify_local(original);
    return result; // Should be 15.0, original unchanged
}

// run: test_param_default_modification() ~= 15.0 [expect-fail]

mat2 multiply_matrices(mat2 a, mat2 b) {
    return a * b;
}

mat2 test_param_default_matrix() {
    // Default qualifier with matrices
    mat2 m1 = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 m2 = mat2(2.0);
    mat2 result = multiply_matrices(m1, m2);
    return result;
}

// run: test_param_default_matrix() ~= mat2(2.0, 4.0, 6.0, 8.0) [expect-fail]

float sum_elements(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_param_default_array() {
    // Default qualifier with arrays
    float[3] data = float[3](1.0, 2.0, 3.0);
    return sum_elements(data);
}

// run: test_param_default_array() ~= 6.0 [expect-fail]

struct Point {
    float x, y;
};

Point move_point(Point p, float dx, float dy) {
    return Point(p.x + dx, p.y + dy);
}

Point test_param_default_struct() {
    // Default qualifier with structs
    Point p = Point(1.0, 2.0);
    return move_point(p, 3.0, 4.0);
}

// run: test_param_default_struct() ~= Point(4.0, 6.0) [expect-fail]
