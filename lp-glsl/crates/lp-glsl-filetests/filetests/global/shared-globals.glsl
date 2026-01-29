// test run
// target riscv32.q32

// ============================================================================
// Shared Globals: Shared global variables across shaders (uniforms in ESSL)
// ============================================================================

// Shared uniform globals - these would need to be declared consistently
// across multiple shaders in a program
uniform float shared_time;
uniform vec3 shared_light_direction;
uniform mat4 shared_view_matrix;
uniform vec4 shared_material_color;
uniform int shared_render_mode;

// These represent globals that must be shared across shaders
// In practice, these would be uniforms that link shaders together

float test_shared_globals_time() {
    // Shared time uniform
    return shared_time + 1.0;
}

// run: test_shared_globals_time() ~= 1.0 [expect-fail]

vec3 test_shared_globals_light() {
    // Shared light direction uniform
    return normalize(shared_light_direction);
}

// run: test_shared_globals_light() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

mat4 test_shared_globals_view() {
    // Shared view matrix uniform
    return shared_view_matrix;
}

// run: test_shared_globals_view() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

vec4 test_shared_globals_material() {
    // Shared material color uniform
    return shared_material_color * 0.5;
}

// run: test_shared_globals_material() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

int test_shared_globals_mode() {
    // Shared render mode uniform
    return shared_render_mode + 1;
}

// run: test_shared_globals_mode() == 1 [expect-fail]

float test_shared_globals_combined() {
    // Combined usage of shared globals
    vec4 transformed_color = shared_view_matrix * shared_material_color;
    float lighting_factor = dot(shared_light_direction, vec3(0.0, 1.0, 0.0));

    return shared_time + transformed_color.x + lighting_factor + float(shared_render_mode);
}

// run: test_shared_globals_combined() ~= 1.0 [expect-fail]
