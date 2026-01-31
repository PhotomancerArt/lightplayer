// test run
// target riscv32.q32

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

// run: test_initialize_undefined_float() ~= 1.0 [expect-fail]

int test_initialize_undefined_int() {
    // Undefined global int - has undefined value
    return undefined_int + 10;
}

// run: test_initialize_undefined_int() == 10 [expect-fail]

uint test_initialize_undefined_uint() {
    // Undefined global uint - has undefined value
    return int(undefined_uint + 1u);
}

// run: test_initialize_undefined_uint() == 1 [expect-fail]

bool test_initialize_undefined_bool() {
    // Undefined global bool - has undefined value
    return undefined_bool || true;
}

// run: test_initialize_undefined_bool() == true [expect-fail]

vec2 test_initialize_undefined_vec2() {
    // Undefined global vec2 - has undefined values
    return undefined_vec2 + vec2(1.0, 1.0);
}

// run: test_initialize_undefined_vec2() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_initialize_undefined_vec3() {
    // Undefined global vec3 - has undefined values
    return undefined_vec3 + vec3(1.0, 1.0, 1.0);
}

// run: test_initialize_undefined_vec3() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

vec4 test_initialize_undefined_vec4() {
    // Undefined global vec4 - has undefined values
    return undefined_vec4 + vec4(1.0, 1.0, 1.0, 1.0);
}

// run: test_initialize_undefined_vec4() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

mat2 test_initialize_undefined_mat2() {
    // Undefined global mat2 - has undefined values
    return undefined_mat2 + mat2(1.0);
}

// run: test_initialize_undefined_mat2() ~= mat2(1.0, 1.0, 1.0, 1.0) [expect-fail]

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

// run: test_initialize_undefined_assign() == 0.0 [expect-fail]

float test_initialize_undefined_after_assign() {
    // Test values after assignment
    test_initialize_undefined_assign();
    return undefined_float + float(undefined_int);
}

// run: test_initialize_undefined_after_assign() ~= 165.0 [expect-fail]
