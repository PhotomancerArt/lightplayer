// test run
// target riscv32.q32

// ============================================================================
// Inout Parameters: Copy in and copy out
// ============================================================================

void increment(inout float value) {
    value = value + 1.0;
}

float test_param_inout_simple() {
    // Inout parameter - copied in and out
    float x = 5.0;
    increment(x);
    return x;
}

// run: test_param_inout_simple() ~= 6.0

void swap(inout float a, inout float b) {
    float temp = a;
    a = b;
    b = temp;
}

void test_param_inout_multiple() {
    // Multiple inout parameters
    float x = 1.0, y = 2.0;
    swap(x, y);
    // Values should be swapped
}

// run: test_param_inout_multiple() == 0.0

void scale_vector(inout vec2 v, float factor) {
    v = v * factor;
}

vec2 test_param_inout_vector() {
    // Inout parameter with vector type
    vec2 vec = vec2(1.0, 2.0);
    scale_vector(vec, 3.0);
    return vec;
}

// run: test_param_inout_vector() ~= vec2(3.0, 6.0)

void add_to_value(inout int value, int amount) {
    value = value + amount;
}

int test_param_inout_int() {
    // Inout parameter with integer type
    int x = 10;
    add_to_value(x, 5);
    return x;
}

// run: test_param_inout_int() == 15

void adjust_components(inout vec3 v) {
    v.x = v.x + 1.0;
    v.y = v.y * 2.0;
    v.z = v.z - 0.5;
}

float test_param_inout_modify_components() {
    // Modify components of inout vector
    vec3 vec = vec3(1.0, 2.0, 3.0);
    adjust_components(vec);
    return vec.x + vec.y + vec.z;
}

// run: test_param_inout_modify_components() ~= 8.5

void toggle(inout bool flag) {
    flag = !flag;
}

bool test_param_inout_bool() {
    // Inout parameter with boolean type
    bool b = false;
    toggle(b);
    return b;
}

// run: test_param_inout_bool() == true

void complex_op(inout float result, float input) {
    result = result * 2.0 + input;
}

float test_param_inout_complex() {
    // Complex inout usage with computation
    float value = 3.0;
    complex_op(value, 4.0);
    return value;
}

// run: test_param_inout_complex() ~= 10.0
