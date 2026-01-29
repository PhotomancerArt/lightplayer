// test run
// target riscv32.q32

// ============================================================================
// Lvalue Required for Out/Inout: out/inout require lvalues (variables)
// ============================================================================

void set_value(out float result) {
    result = 42.0;
}

float test_edge_lvalue_out_variable() {
    // Out parameter with variable (lvalue) - OK
    float value; // Variable is an lvalue
    set_value(value);
    return value;
}

// run: test_edge_lvalue_out_variable() ~= 42.0 [expect-fail]

void set_vector(out vec2 result) {
    result = vec2(1.0, 2.0);
}

vec2 test_edge_lvalue_out_vector() {
    // Out parameter with vector variable - OK
    vec2 vec; // Vector variable is an lvalue
    set_vector(vec);
    return vec;
}

// run: test_edge_lvalue_out_vector() ~= vec2(1.0, 2.0) [expect-fail]

void modify_value(inout float value) {
    value = value * 2.0;
}

float test_edge_lvalue_inout_variable() {
    // Inout parameter with variable - OK
    float x = 5.0; // Variable is lvalue
    modify_value(x);
    return x;
}

// run: test_edge_lvalue_inout_variable() ~= 10.0 [expect-fail]

/*
float test_edge_lvalue_out_expression() {
    // Out parameter with expression - ERROR: expression is not lvalue
    // set_value(5.0 + 3.0); // ERROR: 5.0 + 3.0 is not an lvalue
    // set_value(some_func()); // ERROR: function call result is not lvalue

    return 0.0;
}

// run: test_edge_lvalue_out_expression() ~= 0.0 [expect-fail]
*/

/*
float test_edge_lvalue_inout_literal() {
    // Inout parameter with literal - ERROR: literal is not lvalue
    // modify_value(42.0); // ERROR: 42.0 is not an lvalue

    return 0.0;
}

// run: test_edge_lvalue_inout_literal() ~= 0.0 [expect-fail]
*/

void set_element(out float element) {
    element = 99.0;
}

float test_edge_lvalue_out_array_element() {
    // Out parameter with array element - OK (array element is lvalue)
    float[3] arr;
    set_element(arr[1]); // arr[1] is an lvalue
    return arr[1];
}

// run: test_edge_lvalue_out_array_element() ~= 99.0 [expect-fail]

void scale_component(inout float component) {
    component = component * 3.0;
}

float test_edge_lvalue_inout_swizzle() {
    // Inout parameter with swizzle - OK (swizzle is lvalue)
    vec3 vec = vec3(1.0, 2.0, 3.0);
    scale_component(vec.y); // vec.y is an lvalue
    return vec.y;
}

// run: test_edge_lvalue_inout_swizzle() ~= 6.0 [expect-fail]

/*
float test_edge_lvalue_out_function_call() {
    // Out parameter with function call result - ERROR
    // set_value(get_value()); // ERROR: get_value() is not lvalue

    return 0.0;
}

// run: test_edge_lvalue_out_function_call() ~= 0.0 [expect-fail]
*/

struct Data {
    float value;
};

void set_data(out float field) {
    field = 123.0;
}

float test_edge_lvalue_out_struct_field() {
    // Out parameter with struct field - OK
    Data d;
    set_data(d.value); // d.value is an lvalue
    return d.value;
}

// run: test_edge_lvalue_out_struct_field() ~= 123.0 [expect-fail]

void increment(inout int value) {
    value = value + 1;
}

int test_edge_lvalue_inout_int() {
    // Inout parameter with int variable - OK
    int x = 10;
    increment(x); // x is lvalue
    return x;
}

// run: test_edge_lvalue_inout_int() == 11 [expect-fail]
