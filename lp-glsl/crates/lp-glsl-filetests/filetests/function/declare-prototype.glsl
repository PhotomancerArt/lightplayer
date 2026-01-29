// test run
// target riscv32.q32

// ============================================================================
// Function Prototypes: Forward declarations of functions
// ============================================================================

float test_declare_prototype_simple();

// Simple function prototype declaration
float add_two_floats(float a, float b);

float test_declare_prototype_simple() {
    // Function can be called before definition if prototype exists
    return add_two_floats(3.0, 4.0);
}

// run: test_declare_prototype_simple() ~= 7.0

void test_declare_prototype_void();

// Void function prototype
void test_declare_prototype_void() {
    // Call void function that has prototype
    void_func();
}

// run: test_declare_prototype_void() == 0.0

vec4 test_declare_prototype_vector(vec4 a, vec4 b);

// Vector function prototype with multiple parameters
vec4 test_declare_prototype_vector(vec4 a, vec4 b) {
    return add_vectors(a, b);
}

// run: test_declare_prototype_vector(vec4(1.0), vec4(2.0)) ~= vec4(3.0) [expect-fail]

float test_declare_prototype_multiple();

// Multiple prototypes for the same function (allowed, must match)
float multiply_by_two(float x);
float multiply_by_two(float x); // Duplicate prototype

float test_declare_prototype_multiple() {
    return multiply_by_two(5.0);
}

// run: test_declare_prototype_multiple() ~= 10.0

// ============================================================================
// Function Definitions (implementations for the prototypes above)
// ============================================================================

float add_two_floats(float a, float b) {
    return a + b;
}

void void_func() {
    // Empty void function
}

vec4 add_vectors(vec4 a, vec4 b) {
    return a + b;
}

float multiply_by_two(float x) {
    return x * 2.0;
}
