// test run
// target riscv32.q32

// ============================================================================
// Shared Struct Match: Shared structs must have same definition across shaders
// ============================================================================

// Shared struct definitions - must be identical across shaders
struct Light {
    vec3 position;
    vec3 color;
    float intensity;
};

struct Material {
    vec4 diffuse;
    vec4 specular;
    float shininess;
    bool receives_light;
};

struct Camera {
    vec3 position;
    mat4 view_matrix;
    mat4 projection_matrix;
    float near_plane;
    float far_plane;
};

// Shared uniform structs - definitions must match exactly
uniform Light shared_light;
uniform Material shared_material;
uniform Camera shared_camera;

vec3 test_shared_struct_match_light() {
    // Access shared light struct
    return shared_light.position + shared_light.color * shared_light.intensity;
}

// run: test_shared_struct_match_light() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

vec4 test_shared_struct_match_material() {
    // Access shared material struct
    vec4 final_color = shared_material.diffuse;
    if (shared_material.receives_light) {
        final_color = final_color + shared_material.specular * 0.5;
    }
    return final_color;
}

// run: test_shared_struct_match_material() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat4 test_shared_struct_match_camera_view() {
    // Access shared camera view matrix
    return shared_camera.view_matrix;
}

// run: test_shared_struct_match_camera_view() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

mat4 test_shared_struct_match_camera_projection() {
    // Access shared camera projection matrix
    return shared_camera.projection_matrix;
}

// run: test_shared_struct_match_camera_projection() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_shared_struct_match_camera_planes() {
    // Access shared camera clipping planes
    return shared_camera.near_plane + shared_camera.far_plane;
}

// run: test_shared_struct_match_camera_planes() ~= 0.0 [expect-fail]

vec4 test_shared_struct_match_combined() {
    // Combined access to shared structs
    vec4 world_pos = vec4(shared_camera.position, 1.0);
    vec4 view_pos = shared_camera.view_matrix * world_pos;
    vec4 clip_pos = shared_camera.projection_matrix * view_pos;

    // Apply lighting
    vec3 light_dir = normalize(shared_light.position - shared_camera.position);
    float light_factor = max(dot(light_dir, vec3(0.0, 1.0, 0.0)), 0.0);

    vec4 lit_color = shared_material.diffuse * light_factor * shared_light.color.x;

    return lit_color;
}

// run: test_shared_struct_match_combined() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]
