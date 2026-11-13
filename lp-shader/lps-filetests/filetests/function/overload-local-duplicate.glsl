// test run

// ============================================================================
// Function Overloading: genuinely overloaded local functions (shared name).
// Regression test for the wasm backend, which used to emit one export per IR
// function under its raw GLSL name — overloads produced duplicate export
// names and the module failed wasm validation.
//
// lps-glsl gives each overload its own name in the IR (`pick(vec3)`), so
// every backend sees distinct functions.
// ============================================================================

float pick(float x) {
    return x * 2.0;
}

vec3 pick(vec3 v) {
    return v * 3.0;
}

vec4 pick(vec4 v) {
    return v * 4.0;
}

float test_overload_scalar_vs_vector() {
    // 2*2 + 3*2 + 4*3 = 4 + 6 + 12 = 22
    return pick(2.0) + pick(vec3(1.0, 2.0, 3.0)).y + pick(vec4(1.0, 2.0, 3.0, 4.0)).z;
}

// run: test_overload_scalar_vs_vector() ~= 22.0

float combine(float a) {
    return a + 1.0;
}

float combine(float a, float b) {
    // Calls the one-arg overload from inside another overload.
    return combine(a) + b;
}

float test_overload_arity_and_nested_call() {
    // (1+1) + ((2+1)+3) = 2 + 6 = 8
    return combine(1.0) + combine(2.0, 3.0);
}

// run: test_overload_arity_and_nested_call() ~= 8.0
