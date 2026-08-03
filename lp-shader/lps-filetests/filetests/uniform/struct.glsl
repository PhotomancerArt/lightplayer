// test run

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
layout(binding = 0) uniform Light shared_light;
layout(binding = 0) uniform Material shared_material;
layout(binding = 0) uniform Camera shared_camera;

vec3 test_shared_struct_match_light() {
    // Access shared light struct
    return shared_light.position + shared_light.color * shared_light.intensity;
}

// @unsupported(wgpu.f32)
// run: test_shared_struct_match_light() ~= vec3(0.0, 0.0, 0.0)

vec4 test_shared_struct_match_material() {
    // Access shared material struct
    vec4 final_color = shared_material.diffuse;
    if (shared_material.receives_light) {
        final_color = final_color + shared_material.specular * 0.5;
    }
    return final_color;
}

// @unsupported(wgpu.f32)
// run: test_shared_struct_match_material() ~= vec4(0.0, 0.0, 0.0, 0.0)

mat4 test_shared_struct_match_camera_view() {
    // Access shared camera view matrix
    return shared_camera.view_matrix;
}

// @unsupported(wgpu.f32)
// run: test_shared_struct_match_camera_view() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_shared_struct_match_camera_projection() {
    // Access shared camera projection matrix
    return shared_camera.projection_matrix;
}

// @unsupported(wgpu.f32)
// run: test_shared_struct_match_camera_projection() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

float test_shared_struct_match_camera_planes() {
    // Access shared camera clipping planes
    return shared_camera.near_plane + shared_camera.far_plane;
}

// @unsupported(wgpu.f32)
// run: test_shared_struct_match_camera_planes() ~= 0.0

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

// wgpu.f32: naga validator rejects the assembled unit (std430 uniform blocks / unsized array constructors are invalid on the GPU tier)
// @unsupported(wgpu.f32)
// All uniforms default to zero, so this evaluates normalize(vec3(0)) — 0/0 —
// and then max(NaN, 0.0). The f32 targets legitimately disagree about the
// result: wasm.f32 returns vec4(NaN..), interp.f32 returns vec4(0..).
//
// G1 answered this (2026-07-31): the authority is the product spec, not IEEE and
// not silicon. docs/design/float.md §5 classifies BOTH operations as
// **Unspecified** — `max` with a NaN operand (IEEE defines competing operations
// and GLSL declares it undefined) and `normalize(vec3(0))` (a GLSL-undefined
// library input). Neither target is wrong.
//
// So the f32 expectation is **retired, not carried** (2026-08-02): float.md §5
// rule 3 says unspecified behavior is never asserted, and `@broken` asserted it
// while calling the disagreement a bug. Scoping the directive to `run[q32]:`
// states the real rule — Q32 has one defined answer here and must keep hitting
// it; the f32 tiers are free, so nothing asks them. Do not restore a `run[f32]:`
// channel for this function: any value it could name would be one target's
// opinion promoted to a spec.
//
// The five directives above stay unscoped on purpose — reading a zero-defaulted
// uniform is Guaranteed in both modes, and only *this* function reaches the
// undefined pair.
// run[q32]: test_shared_struct_match_combined() ~= vec4(0.0, 0.0, 0.0, 0.0)
