// test run

// ============================================================================
// Undefined Global Initialization: Global variables without initialization
// ============================================================================

float undefined_float;
int undefined_int;
uint undefined_uint;
bool undefined_bool;
vec2 undefined_vec2;
vec3 undefined_vec3;
vec4 undefined_vec4;
mat2 undefined_mat2;

float test_initialize_undefined_float() {
    // Undefined global float - has undefined value
    // Reading undefined values produces undefined behavior
    return undefined_float + 1.0;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_float() ~= 1.0

int test_initialize_undefined_int() {
    // Undefined global int - has undefined value
    return undefined_int + 10;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_int() == 10

uint test_initialize_undefined_uint() {
    // Undefined global uint - has undefined value
    return int(undefined_uint + 1u);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_uint() == 1

bool test_initialize_undefined_bool() {
    // Undefined global bool - has undefined value
    return undefined_bool || true;
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_bool() == true

vec2 test_initialize_undefined_vec2() {
    // Undefined global vec2 - has undefined values
    return undefined_vec2 + vec2(1.0, 1.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_vec2() ~= vec2(1.0, 1.0)

vec3 test_initialize_undefined_vec3() {
    // Undefined global vec3 - has undefined values
    return undefined_vec3 + vec3(1.0, 1.0, 1.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_vec3() ~= vec3(1.0, 1.0, 1.0)

vec4 test_initialize_undefined_vec4() {
    // Undefined global vec4 - has undefined values
    return undefined_vec4 + vec4(1.0, 1.0, 1.0, 1.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_vec4() ~= vec4(1.0, 1.0, 1.0, 1.0)

mat2 test_initialize_undefined_mat2() {
    // Undefined global mat2 - has undefined values
    return undefined_mat2 + mat2(1.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_mat2() ~= mat2(1.0, 1.0, 1.0, 1.0)

void test_initialize_undefined_assign() {
    // Assign to undefined globals to give them defined values
    undefined_float = 42.0;
    undefined_int = 123;
    undefined_uint = 456u;
    undefined_bool = true;
    undefined_vec2 = vec2(1.0, 2.0);
    undefined_vec3 = vec3(1.0, 2.0, 3.0);
    undefined_vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    undefined_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_assign() == 0.0

float test_initialize_undefined_after_assign() {
    // Test values after assignment
    test_initialize_undefined_assign();
    return undefined_float + float(undefined_int);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_initialize_undefined_after_assign() ~= 165.0
