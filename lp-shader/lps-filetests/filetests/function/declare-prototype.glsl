// test run

// ============================================================================
// Function Prototypes: Forward declarations of functions
// ============================================================================

float test_declare_prototype_simple();

// Simple function prototype declaration
float add_two_floats(float a, float b);

// Helpers defined later (Naga needs a forward declaration when the body appears first in filtered sources)
void void_func();
vec4 add_vectors(vec4 a, vec4 b);

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

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_declare_prototype_vector(vec4(1.0), vec4(2.0)) ~= vec4(3.0)

float test_declare_prototype_multiple();

// Prototype before definition (GLSL also allows duplicate matching prototypes; Naga rejects duplicates)
float multiply_by_two(float x);

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
