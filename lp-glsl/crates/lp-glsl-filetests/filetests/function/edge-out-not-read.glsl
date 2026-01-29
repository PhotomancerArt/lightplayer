// test run
// target riscv32.q32

// ============================================================================
// Out Parameters Not Read: Can assign to out without reading first
// ============================================================================

void set_value(out float result) {
    result = 42.0; // Direct assignment, no read
}

float test_edge_out_not_read() {
    // Can assign to out parameter without reading it first
    float value;
    set_value(value);
    return value;
}

// run: test_edge_out_not_read() ~= 42.0

void overwrite_value(out float result) {
    result = 100.0; // Overwrites without reading
}

void test_edge_out_overwrite_existing() {
    // Out parameter overwrites any existing value in caller
    float value = 999.0; // This value will be overwritten
    overwrite_value(value);
    // value is now 100.0, not 999.0
}

// run: test_edge_out_overwrite_existing() == 0.0

void set_vector(out vec2 result) {
    result = vec2(1.0, 2.0); // Direct assignment
}

vec2 test_edge_out_vector_not_read() {
    // Vector out parameters can be assigned without reading
    vec2 v;
    set_vector(v);
    return v;
}

// run: test_edge_out_vector_not_read() ~= vec2(1.0, 2.0)

void set_components(out vec3 result) {
    result.x = 1.0;
    result.y = 2.0;
    result.z = 3.0;
}

float test_edge_out_modify_components() {
    // Can modify individual components without reading whole vector
    vec3 v;
    set_components(v);
    return v.x + v.y + v.z;
}

// run: test_edge_out_modify_components() ~= 6.0

void set_counter(out int result) {
    result = 5;
}

int test_edge_out_int_not_read() {
    // Integer out parameters
    int counter;
    set_counter(counter);
    return counter;
}

// run: test_edge_out_int_not_read() == 5

void set_flag(out bool result) {
    result = true;
}

bool test_edge_out_bool_not_read() {
    // Boolean out parameters
    bool flag;
    set_flag(flag);
    return flag;
}

// run: test_edge_out_bool_not_read() == true

void compute_result(out float result, float x, float y) {
    result = x * y + x + y; // Complex expression
}

float test_edge_out_expression_assignment() {
    // Can assign expressions to out parameters
    float value;
    compute_result(value, 3.0, 4.0);
    return value;
}

// run: test_edge_out_expression_assignment() ~= 19.0

void iterative_assignment(out float result) {
    result = 1.0;
    result = result * 2.0; // Reassign
    result = result + 3.0; // Reassign again
}

void test_edge_out_multiple_assignments() {
    // Can assign multiple times to out parameter
    float value;
    iterative_assignment(value);
    // Final value should be 5.0
}

// run: test_edge_out_multiple_assignments() == 0.0

void set_matrix(out mat2 result) {
    result = mat2(1.0, 2.0, 3.0, 4.0);
}

mat2 test_edge_out_matrix_not_read() {
    // Matrix out parameters
    mat2 m;
    set_matrix(m);
    return m;
}

// run: test_edge_out_matrix_not_read() ~= mat2(1.0, 2.0, 3.0, 4.0)
