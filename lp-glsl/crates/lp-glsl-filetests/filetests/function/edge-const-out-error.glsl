// test run
// target riscv32.q32

// ============================================================================
// Const with Out/Inout Error: const cannot be used with out or inout
// Note: These are expected to be COMPILE ERRORS
// ============================================================================

void good_const_in(const float x) {
    // x is read-only
}

float test_edge_const_out_error() {
    // const cannot be used with out - this should be a compile error
    // void bad_const_out(const out float x) { // ERROR: const out not allowed
    //     x = 1.0;
    // }

    // Valid: const in (which is the same as const)
    good_const_in(5.0);
    return 1.0;
}

// run: test_edge_const_out_error() ~= 1.0 [expect-fail]

/*
float test_edge_const_inout_error() {
    // const cannot be used with inout - this should be a compile error
    // void bad_const_inout(const inout float x) { // ERROR: const inout not allowed
    //     x = x + 1.0;
    // }

    return 0.0;
}

// run: test_edge_const_inout_error() ~= 0.0 [expect-fail]
*/

void const_in_parameter(const float x) {
    float local = x * 2.0; // OK: can read const parameter
    // x = 5.0; // ERROR: cannot assign to const parameter
}

float test_edge_const_only_with_in() {
    // const can only be used with in parameters
    const_in_parameter(3.0);
    return 2.0;
}

// run: test_edge_const_only_with_in() ~= 2.0 [expect-fail]

void explicit_const_in(const in float x) {
    // x is read-only
}

float test_edge_const_in_explicit() {
    // const in is explicit read-only
    explicit_const_in(42.0);
    return 3.0;
}

// run: test_edge_const_in_explicit() ~= 3.0 [expect-fail]

/*
float test_edge_const_multiple_qualifiers() {
    // Cannot combine const with out or inout
    // void const_out(const out float x) { } // ERROR
    // void const_inout(const inout float x) { } // ERROR
    // void out_const(out const float x) { } // ERROR
    // void inout_const(inout const float x) { } // ERROR

    return 0.0;
}

// run: test_edge_const_multiple_qualifiers() ~= 0.0 [expect-fail]
*/

void const_array(const float[3] arr) {
    float sum = arr[0] + arr[1] + arr[2];
    // Cannot modify arr elements
}

float test_edge_const_array() {
    // const can be used with array parameters
    float[3] data = float[3](1.0, 2.0, 3.0);
    const_array(data);
    return 4.0;
}

// run: test_edge_const_array() ~= 4.0 [expect-fail]

struct Point {
    float x, y;
};

void const_struct(const Point p) {
    float dist = sqrt(p.x * p.x + p.y * p.y);
    // Cannot modify p
}

float test_edge_const_struct() {
    // const can be used with struct parameters
    Point pt = Point(3.0, 4.0);
    const_struct(pt);
    return 5.0;
}

// run: test_edge_const_struct() ~= 5.0 [expect-fail]
