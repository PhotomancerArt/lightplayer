// test run
// target riscv32.q32

// ============================================================================
// Uniform No Init Error: Uniform globals cannot be initialized in shader code
// ============================================================================

// Valid uniform declarations (no initialization)
uniform float time;
uniform int count;
uniform uint seed;
uniform bool enabled;
uniform vec2 position;
uniform vec3 color;
uniform vec4 data;
uniform mat4 transform;

// These would typically be compile errors in GLSL:
// uniform float bad_time = 1.0;     // Error: uniforms cannot be initialized
// uniform int bad_count = 5;        // Error: uniforms cannot be initialized
// uniform vec3 bad_color = vec3(1.0, 0.0, 0.0);  // Error: uniforms cannot be initialized

// Note: Some GLSL implementations may allow uniform initialization,
// but the GLSL specification typically does not support it.

float test_uniform_no_init_float() {
    // Uniform float without initialization
    return time + 1.0;
}

// run: test_uniform_no_init_float() ~= 1.0 [expect-fail]

int test_uniform_no_init_int() {
    // Uniform int without initialization
    return count + 10;
}

// run: test_uniform_no_init_int() == 10 [expect-fail]

uint test_uniform_no_init_uint() {
    // Uniform uint without initialization
    return int(seed + 100u);
}

// run: test_uniform_no_init_uint() == 100 [expect-fail]

bool test_uniform_no_init_bool() {
    // Uniform bool without initialization
    return enabled || true;
}

// run: test_uniform_no_init_bool() == true [expect-fail]

vec2 test_uniform_no_init_vec2() {
    // Uniform vec2 without initialization
    return position + vec2(1.0, 1.0);
}

// run: test_uniform_no_init_vec2() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_uniform_no_init_vec3() {
    // Uniform vec3 without initialization
    return color * 2.0;
}

// run: test_uniform_no_init_vec3() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

vec4 test_uniform_no_init_vec4() {
    // Uniform vec4 without initialization
    return data;
}

// run: test_uniform_no_init_vec4() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat4 test_uniform_no_init_mat4() {
    // Uniform mat4 without initialization
    return transform;
}

// run: test_uniform_no_init_mat4() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_uniform_no_init_calculations() {
    // Calculations with uninitialized uniforms
    float result = time;
    result = result + float(count);
    result = result + float(seed);
    result = result + position.x + position.y;
    result = result + color.x + color.y + color.z;

    return result;
}

// run: test_uniform_no_init_calculations() ~= 0.0 [expect-fail]
