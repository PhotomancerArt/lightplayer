// test run
// target riscv32.q32

// ============================================================================
// Return Type Must Match: Return value must match declared return type
// ============================================================================

float get_pi() {
    return 3.14159; // OK: float matches float
}

float test_edge_return_type_match_float() {
    // Return type must match declaration
    return get_pi();
}

// run: test_edge_return_type_match_float() ~= 3.14159 [expect-fail]

int get_answer() {
    return 42; // OK: int matches int
}

int test_edge_return_type_match_int() {
    // Integer return type
    return get_answer();
}

// run: test_edge_return_type_match_int() == 42 [expect-fail]

vec2 get_origin() {
    return vec2(0.0, 0.0); // OK: vec2 matches vec2
}

vec2 test_edge_return_type_match_vector() {
    // Vector return type
    return get_origin();
}

// run: test_edge_return_type_match_vector() ~= vec2(0.0, 0.0) [expect-fail]

void do_nothing() {
    // OK: no return statement needed for void
}

void test_edge_return_type_match_void() {
    // Void return type
    do_nothing();
}

// run: test_edge_return_type_match_void() == 0.0 [expect-fail]

/*
float test_edge_return_type_mismatch() {
    // Return type mismatch - should be compile error
    // return 0.0;
    return 0.0;
}

// run: test_edge_return_type_mismatch() ~= 0.0 [expect-fail]
*/

float int_to_float() {
    return float(42); // Explicit cast
}

float test_edge_return_type_convertible() {
    // Convertible types may be allowed
    return int_to_float();
}

// run: test_edge_return_type_convertible() ~= 42.0 [expect-fail]

/*
void test_edge_return_value_in_void() {
    // Void functions cannot return values - compile error
    bad_void();
}

// run: test_edge_return_value_in_void() == 0.0 [expect-fail]
*/

float[3] get_array() {
    return float[3](1.0, 2.0, 3.0); // OK: float[3] matches float[3]
}

float test_edge_return_type_array() {
    // Array return types must match exactly
    float[3] arr = get_array();
    return arr[0] + arr[1] + arr[2];
}

// run: test_edge_return_type_array() ~= 6.0 [expect-fail]

/*
float test_edge_return_type_array_mismatch() {
    // Array size mismatch - compile error
    return 0.0;
}

// run: test_edge_return_type_array_mismatch() ~= 0.0 [expect-fail]
*/

struct Point {
    float x, y;
};

Point get_point() {
    return Point(1.0, 2.0); // OK: Point matches Point
}

Point test_edge_return_type_struct() {
    // Struct return types must match
    Point p = get_point();
    return p;
}

// run: test_edge_return_type_struct() ~= Point(1.0, 2.0) [expect-fail]

mat2 get_identity() {
    return mat2(1.0); // OK: mat2 matches mat2
}

mat2 test_edge_return_type_matrix() {
    // Matrix return types must match
    return get_identity();
}

// run: test_edge_return_type_matrix() ~= mat2(1.0, 0.0, 0.0, 1.0) [expect-fail]

bool is_even(int x) {
    return (x % 2) == 0; // OK: bool expression for bool return
}

bool test_edge_return_type_bool() {
    // Boolean return types
    return is_even(4);
}

// run: test_edge_return_type_bool() == true [expect-fail]

float absolute_value(float x) {
    if (x >= 0.0) {
        return x; // OK: float
    } else {
        return -x; // OK: float
    }
}

float test_edge_return_type_multiple_returns() {
    // All return statements must match return type
    return absolute_value(-5.0);
}

// run: test_edge_return_type_multiple_returns() ~= 5.0 [expect-fail]
