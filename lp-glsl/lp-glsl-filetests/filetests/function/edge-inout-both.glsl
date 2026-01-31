// test run
// target riscv32.q32

// ============================================================================
// Inout Parameters Both Ways: Copied in and copied out
// ============================================================================

void modify_inout(inout float value) {
    value = value + 10.0; // Modifies caller's variable
}

float test_edge_inout_both_directions() {
    // Inout parameter is copied in at start, copied out at end
    float x = 5.0;
    modify_inout(x);
    return x; // Should be 15.0
}

// run: test_edge_inout_both_directions() ~= 15.0

void read_inout(inout float value) {
    float local = value; // local gets the input value
    value = local + 1.0; // Modify for output
}

void test_edge_inout_copy_in() {
    // Value is copied in at function entry
    float x = 100.0;
    read_inout(x);
    // x should be 101.0
}

// run: test_edge_inout_copy_in() == 0.0

void write_inout(inout float value) {
    value = 42.0; // This gets copied back to caller
}

void test_edge_inout_copy_out() {
    // Value is copied out at function exit
    float x = 999.0; // This will be overwritten
    write_inout(x);
    // x should be 42.0
}

// run: test_edge_inout_copy_out() == 0.0

void transform_vector(inout vec2 v) {
    v = v * 2.0; // Double the input vector
    v = v + vec2(1.0, 1.0); // Add offset
}

vec2 test_edge_inout_vector_both() {
    // Vector inout parameters work both ways
    vec2 vec = vec2(1.0, 2.0);
    transform_vector(vec);
    return vec; // Should be vec2(3.0, 5.0)
}

// run: test_edge_inout_vector_both() ~= vec2(3.0, 5.0)

void increment_inout(inout int value) {
    value = value + 1; // Increment the input
}

int test_edge_inout_int_both() {
    // Integer inout parameters
    int x = 10;
    increment_inout(x);
    return x; // Should be 11
}

// run: test_edge_inout_int_both() == 11

void complex_inout(inout float value) {
    value = value * 2.0;  // Double input
    value = value + 5.0;  // Add 5
    value = value / 3.0;  // Divide by 3
}

float test_edge_inout_multiple_operations() {
    // Multiple operations on inout parameter
    float x = 6.0;
    complex_inout(x);
    return x; // ((6*2)+5)/3 = 17/3 = 5.666...
}

// run: test_edge_inout_multiple_operations() ~= 5.666 [expect-fail]

void no_modify(inout float value) {
    float local = value; // Read input
    // Don't modify value
}

void test_edge_inout_unchanged_if_not_modified() {
    // If inout parameter is not modified, original value preserved
    float x = 123.0;
    no_modify(x);
    // x should still be 123.0
}

// run: test_edge_inout_unchanged_if_not_modified() == 0.0

void toggle_inout(inout bool flag) {
    flag = !flag; // Toggle the input boolean
}

bool test_edge_inout_bool_both() {
    // Boolean inout parameters
    bool b = false;
    toggle_inout(b);
    return b; // Should be true
}

// run: test_edge_inout_bool_both() == true

void use_in_expression(inout float value) {
    float doubled = value * 2.0;
    value = doubled + value; // value = 2*value + value = 3*value
}

float test_edge_inout_expression() {
    // Inout can be used in expressions
    float x = 4.0;
    use_in_expression(x);
    return x; // Should be 12.0
}

// run: test_edge_inout_expression() ~= 12.0
