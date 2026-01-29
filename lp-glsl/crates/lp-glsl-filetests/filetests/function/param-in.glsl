// test run
// target riscv32.q32

// ============================================================================
// In Parameters: Default parameter qualifier (copy in only)
// ============================================================================

float add_in(in float a, in float b) {
    return a + b;
}

float test_param_in_explicit() {
    // Explicit 'in' qualifier
    return add_in(3.0, 4.0);
}

// run: test_param_in_explicit() ~= 7.0

float multiply_in(float a, float b) {
    return a * b;
}

float test_param_in_implicit() {
    // Implicit 'in' qualifier (default)
    return multiply_in(3.0, 4.0);
}

// run: test_param_in_implicit() ~= 12.0

float modify_and_return(in float x) {
    x = x + 1.0;  // This modifies local copy only
    return x;
}

float test_param_in_modify_local() {
    // In parameters can be modified inside function (affects only local copy)
    return modify_and_return(5.0);
}

// run: test_param_in_modify_local() ~= 6.0

vec2 add_vectors_in(in vec2 a, in vec2 b) {
    return a + b;
}

vec2 test_param_in_vector() {
    // In parameters with vector types
    return add_vectors_in(vec2(1.0, 2.0), vec2(3.0, 4.0));
}

// run: test_param_in_vector() ~= vec2(4.0, 6.0)

int add_ints_in(in int a, in int b) {
    return a + b;
}

int test_param_in_int() {
    // In parameters with integer types
    return add_ints_in(10, 20);
}

// run: test_param_in_int() == 30

uint add_uints_in(in uint a, in uint b) {
    return a + b;
}

uint test_param_in_uint() {
    // In parameters with unsigned integer types
    return add_uints_in(100u, 200u);
}

// run: test_param_in_uint() == 300u

bool and_bools_in(in bool a, in bool b) {
    return a && b;
}

bool test_param_in_bool() {
    // In parameters with boolean types
    return and_bools_in(true, false);
}

// run: test_param_in_bool() == false

vec3 process_vector_in(in vec3 v) {
    v.x = v.x * 2.0;
    v.y = v.y + 1.0;
    v.z = v.z - 0.5;
    return v;
}

vec3 test_param_in_modify_components() {
    // Modify individual components of in parameter
    return process_vector_in(vec3(1.0, 2.0, 3.0));
}

// run: test_param_in_modify_components() ~= vec3(2.0, 3.0, 2.5)
