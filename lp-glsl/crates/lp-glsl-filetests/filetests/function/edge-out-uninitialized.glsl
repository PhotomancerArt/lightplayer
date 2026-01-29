// test run
// target riscv32.q32

// ============================================================================
// Out Parameters Uninitialized: out parameters start uninitialized
// ============================================================================

void test_uninitialized(out float result) {
    // result is uninitialized here - would be undefined behavior to read
    result = 42.0; // Must assign before function returns
}

float test_edge_out_uninitialized() {
    // Out parameters are uninitialized when function starts
    float value;
    test_uninitialized(value);
    return value;
}

// run: test_edge_out_uninitialized() ~= 42.0

void forget_to_assign(out float result) {
    // result remains uninitialized
    float dummy = 1.0; // Do something else
}

void test_edge_out_not_assigned() {
    // If out parameter is not assigned, caller gets undefined value
    float value;
    forget_to_assign(value);
    // value is now undefined - this is undefined behavior
}

// run: test_edge_out_not_assigned() == 0.0

void assign_conditional(out float result, bool condition) {
    if (condition) {
        result = 1.0;
    } else {
        result = 2.0;
    }
    // All paths must assign to result
}

float test_edge_out_partial_assignment() {
    // Out parameter must be fully assigned before return
    float value;
    assign_conditional(value, true);
    return value;
}

// run: test_edge_out_partial_assignment() ~= 1.0

void create_vector(out vec2 result) {
    // result is completely uninitialized
    result = vec2(3.0, 4.0); // Must assign entire vector
}

vec2 test_edge_out_vector_uninitialized() {
    // Vector out parameters are also uninitialized
    vec2 v;
    create_vector(v);
    return v;
}

// run: test_edge_out_vector_uninitialized() ~= vec2(3.0, 4.0)

void multiple_out(out float a, out float b, out float c) {
    a = 1.0;
    b = 2.0;
    c = 3.0;
}

void test_edge_out_multiple_uninitialized() {
    // Multiple out parameters are all uninitialized
    float x, y, z;
    multiple_out(x, y, z);
    // All must be assigned
}

// run: test_edge_out_multiple_uninitialized() == 0.0

void bad_read(out float result) {
    float temp = result; // UNDEFINED: reading uninitialized out parameter
    result = temp + 1.0; // This would be undefined
}

float test_edge_out_read_before_write() {
    // Reading uninitialized out parameter - undefined behavior
    float value = 999.0; // This should be ignored
    // bad_read(value); // This would be undefined behavior
    return 0.0;
}

// run: test_edge_out_read_before_write() ~= 0.0

void set_int(out int result) {
    result = 42;
}

int test_edge_out_int_uninitialized() {
    // Integer out parameters are also uninitialized
    int value;
    set_int(value);
    return value;
}

// run: test_edge_out_int_uninitialized() == 42

void set_bool(out bool result) {
    result = true;
}

bool test_edge_out_bool_uninitialized() {
    // Boolean out parameters are uninitialized
    bool flag;
    set_bool(flag);
    return flag;
}

// run: test_edge_out_bool_uninitialized() == true
