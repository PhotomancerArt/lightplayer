// test run
// target riscv32.q32

// ============================================================================
// Uniform Global Initialization: Global variables with uniform qualifier initialized
// ============================================================================

// Note: Uniform initialization may not be allowed in standard GLSL
// These tests may need to be error tests or may not compile

// uniform float time_init = 0.0;      // May be compile error
// uniform int count_init = 0;         // May be compile error
// uniform vec2 position_init = vec2(0.0, 0.0);  // May be compile error

uniform float time;
uniform int count;
uniform vec2 position;
uniform vec3 color;
uniform mat4 transform;

float test_initialize_uniform_float() {
    // Uniform global float (no initialization)
    return time + 1.0;
}

// run: test_initialize_uniform_float() ~= 1.0 [expect-fail]

int test_initialize_uniform_int() {
    // Uniform global int (no initialization)
    return count + 5;
}

// run: test_initialize_uniform_int() == 5 [expect-fail]

vec2 test_initialize_uniform_vec2() {
    // Uniform global vec2 (no initialization)
    return position + vec2(1.0, 1.0);
}

// run: test_initialize_uniform_vec2() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_initialize_uniform_vec3() {
    // Uniform global vec3 (no initialization)
    return color * 2.0;
}

// run: test_initialize_uniform_vec3() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

mat4 test_initialize_uniform_mat4() {
    // Uniform global mat4 (no initialization)
    return transform;
}

// run: test_initialize_uniform_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_initialize_uniform_usage() {
    // Uniform globals used in calculations
    float scaled_time = time * 2.0;
    int doubled_count = count * 2;
    vec2 offset_pos = position + vec2(0.5, 0.5);

    return scaled_time + float(doubled_count) + offset_pos.x + offset_pos.y;
}

// run: test_initialize_uniform_usage() ~= 2.0 [expect-fail]
