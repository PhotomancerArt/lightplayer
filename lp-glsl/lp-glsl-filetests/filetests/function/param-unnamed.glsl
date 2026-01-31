// test run
// target riscv32.q32

// ============================================================================
// Unnamed Parameters: Parameters without names in declarations
// ============================================================================

// Prototype with unnamed parameters
float add(float, float);

float add(float a, float b) { // Definition with named parameters
    return a + b;
}

float test_param_unnamed_simple() {
    // Unnamed parameters in function declaration
    return add(3.0, 4.0);
}

// run: test_param_unnamed_simple() ~= 7.0

// Prototype with unnamed parameters
void process(float, int);

void process(float value, int count) { // Definition with names
    // Process value 'count' times
}

void test_param_unnamed_void() {
    // Unnamed parameters in void function
    process(5.0, 3);
}

// run: test_param_unnamed_void() == 0.0

// First param unnamed, second named
float multiply(float, int count);

float multiply(float factor, int count) { // Definition with both named
    float result = 1.0;
    for (int i = 0; i < count; i++) {
        result = result * factor;
    }
    return result;
}

float test_param_unnamed_mixed() {
    // Mix of named and unnamed in prototype
    return multiply(2.0, 3); // 2^3 = 8
}

// run: test_param_unnamed_mixed() ~= 8.0

// Both parameters unnamed in prototype
vec2 combine(vec2, vec2);

vec2 combine(vec2 a, vec2 b) { // Named in definition
    return a + b;
}

vec2 test_param_unnamed_vector() {
    // Unnamed parameters with vectors
    return combine(vec2(1.0, 2.0), vec2(3.0, 4.0));
}

// run: test_param_unnamed_vector() ~= vec2(4.0, 6.0)

// Prototype
float compute(float, float, float);

float compute(float a, float b, float c) { // Definition gives them names
    return a * b + c;
}

float test_param_unnamed_all_unnamed() {
    // All parameters unnamed in both prototype and definition
    return compute(2.0, 3.0, 4.0); // 2*3 + 4 = 10
}

// run: test_param_unnamed_all_unnamed() ~= 10.0

// Prototype
int max_value(int, int);

int max_value(int a, int b) { // Definition
    return a > b ? a : b;
}

int test_param_unnamed_int() {
    // Unnamed parameters with integers
    return max_value(5, 8);
}

// run: test_param_unnamed_int() == 8

// Prototype
bool both_true(bool, bool);

bool both_true(bool a, bool b) { // Definition
    return a && b;
}

bool test_param_unnamed_bool() {
    // Unnamed parameters with booleans
    return both_true(true, false);
}

// run: test_param_unnamed_bool() == false

// Forward declare
float complex_calc(float, int, bool);

float test_param_unnamed_forward_declare() {
    // Forward declaration with unnamed parameters
    // Use before definition
    float result1 = complex_calc(2.0, 3, true);

    // Definition
    float result2 = complex_calc(2.0, 3, true);
    return result1 + result2; // 8 + 8 = 16
}

// Definition
float complex_calc(float base, int exp, bool enable) {
    if (!enable) return 0.0;
    float result = 1.0;
    for (int i = 0; i < exp; i++) {
        result = result * base;
    }
    return result;
}

// run: test_param_unnamed_forward_declare() ~= 16.0
