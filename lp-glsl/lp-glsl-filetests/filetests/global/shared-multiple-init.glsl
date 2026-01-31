// test run
// target riscv32.q32

// ============================================================================
// Shared Multiple Init: Shared globals with multiple initializers across shaders
// ============================================================================

// Shared const globals - initializers must be identical across shaders
const float SHARED_PI = 3.14159;
const vec3 SHARED_UP = vec3(0.0, 1.0, 0.0);
const mat2 SHARED_ROTATE_90 = mat2(0.0, -1.0, 1.0, 0.0);

// Note: In GLSL, shared globals (uniforms) cannot have initializers in the shader.
// Const globals can have initializers, but they must be constant expressions.
// For shared globals across shaders, the initializers must be identical.

float test_shared_multiple_init_pi() {
    // Access shared const PI
    return SHARED_PI * 2.0;
}

// run: test_shared_multiple_init_pi() ~= 6.28318 [expect-fail]

vec3 test_shared_multiple_init_up() {
    // Access shared const UP vector
    return SHARED_UP;
}

// run: test_shared_multiple_init_up() ~= vec3(0.0, 1.0, 0.0) [expect-fail]

mat2 test_shared_multiple_init_rotate() {
    // Access shared const rotation matrix
    return SHARED_ROTATE_90;
}

// run: test_shared_multiple_init_rotate() ~= mat2(0.0, -1.0, 1.0, 0.0) [expect-fail]

vec2 test_shared_multiple_init_apply_rotate() {
    // Apply shared rotation to a vector
    vec2 input_vec = vec2(1.0, 0.0);
    vec2 rotated = SHARED_ROTATE_90 * input_vec;
    return rotated;
}

// run: test_shared_multiple_init_apply_rotate() ~= vec2(0.0, -1.0) [expect-fail]

float test_shared_multiple_init_trig() {
    // Use shared PI in trigonometric calculations
    float angle = SHARED_PI / 4.0;  // 45 degrees
    return angle * 2.0;
}

// run: test_shared_multiple_init_trig() ~= 1.570795 [expect-fail]

vec3 test_shared_multiple_init_cross() {
    // Use shared UP vector in cross product
    vec3 right = vec3(1.0, 0.0, 0.0);
    vec3 forward = cross(SHARED_UP, right);
    return forward;
}

// run: test_shared_multiple_init_cross() ~= vec3(0.0, 0.0, 1.0) [expect-fail]
