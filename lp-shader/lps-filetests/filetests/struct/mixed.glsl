// test run

// ============================================================================
// Mixed member types: scalars, vectors, matrices, nested structs (game-style)
// ============================================================================

struct Vertex {
    vec3 position; // vector
    vec3 normal;   // vector
    float u;       // scalar
    float v;       // scalar
};

struct Material {
    vec3 ambient;    // vector
    vec3 diffuse;    // vector
    float shininess; // scalar
    int flags;       // scalar
};

struct SceneObject {
    vec3 position; // vector
    mat3 rotation; // matrix
    float scale;   // scalar
    Material mat;  // nested struct
};

// --- Mixed construction ----------------------------------------------------

float test_mixed_construct_vertex() {
    Vertex v = Vertex(vec3(1.0, 2.0, 3.0), vec3(0.0, 1.0, 0.0), 0.5, 0.5);
    return v.position.x + v.u; // 1.0 + 0.5 = 1.5
}

// @unimplemented(jit.q32)
// run: test_mixed_construct_vertex() ~= 1.5

float test_mixed_construct_material_shininess() {
    Material m = Material(vec3(0.1, 0.2, 0.3), vec3(0.4, 0.5, 0.6), 64.0, 3);
    return m.shininess + float(m.flags); // 64 + 3 = 67
}

// @unimplemented(jit.q32)
// run: test_mixed_construct_material_shininess() ~= 67.0

vec3 test_mixed_construct_scene_position() {
    SceneObject o = SceneObject(
        vec3(10.0, 20.0, 30.0),
        mat3(1.0),
        1.0,
        Material(vec3(0.0), vec3(1.0, 0.0, 0.0), 8.0, 0));
    return o.position;
}

// @unimplemented(jit.q32)
// run: test_mixed_construct_scene_position() ~= vec3(10.0, 20.0, 30.0)

// --- Mixed read --------------------------------------------------------------

float test_mixed_read_vertex_normal_y() {
    Vertex v = Vertex(vec3(0.0), vec3(0.25, 0.75, 0.0), 0.0, 0.0);
    return v.normal.y; // 0.75
}

// @unimplemented(jit.q32)
// run: test_mixed_read_vertex_normal_y() ~= 0.75

int test_mixed_read_material_flags() {
    Material m = Material(vec3(0.0), vec3(0.0), 1.0, 42);
    return m.flags;
}

// @unimplemented(jit.q32)
// run: test_mixed_read_material_flags() == 42

float test_mixed_read_scene_rotation_identity() {
    SceneObject o = SceneObject(vec3(0.0), mat3(1.0), 1.0, Material(vec3(0.0), vec3(0.0), 0.0, 0));
    return o.rotation[1][1]; // identity
}

// @unimplemented(jit.q32)
// run: test_mixed_read_scene_rotation_identity() ~= 1.0

vec3 test_mixed_read_nested_material_diffuse() {
    SceneObject o = SceneObject(
        vec3(0.0),
        mat3(1.0),
        1.0,
        Material(vec3(0.0), vec3(2.0, 3.0, 4.0), 0.0, 0));
    return o.mat.diffuse;
}

// @unimplemented(jit.q32)
// run: test_mixed_read_nested_material_diffuse() ~= vec3(2.0, 3.0, 4.0)

// --- Mixed write -------------------------------------------------------------

float test_mixed_write_vertex_uv() {
    Vertex v = Vertex(vec3(0.0), vec3(0.0), 0.0, 0.0);
    v.u = 0.25;
    v.v = 0.75;
    return v.u + v.v; // 1.0
}

// @unimplemented(jit.q32)
// run: test_mixed_write_vertex_uv() ~= 1.0

float test_mixed_write_material_shininess() {
    Material m = Material(vec3(0.0), vec3(0.0), 1.0, 0);
    m.shininess = 128.0;
    return m.shininess;
}

// @unimplemented(jit.q32)
// run: test_mixed_write_material_shininess() ~= 128.0

float test_mixed_write_scene_scale_and_nested() {
    SceneObject o = SceneObject(
        vec3(0.0),
        mat3(1.0),
        1.0,
        Material(vec3(0.0), vec3(0.0), 0.0, 0));
    o.scale = 3.0;
    o.mat.ambient = vec3(1.0, 0.0, 0.0);
    return o.scale + o.mat.ambient.x; // 4.0
}

// @unimplemented(jit.q32)
// run: test_mixed_write_scene_scale_and_nested() ~= 4.0

float test_mixed_write_rotation_whole() {
    SceneObject o = SceneObject(vec3(0.0), mat3(0.0), 1.0, Material(vec3(0.0), vec3(0.0), 0.0, 0));
    o.rotation = mat3(5.0); // diagonal 5 (whole-matrix store; element stores via struct are unsupported)
    return o.rotation[0][0];
}

// @unimplemented(jit.q32)
// run: test_mixed_write_rotation_whole() ~= 5.0

// --- Mixed whole-struct assignment -------------------------------------------

float test_mixed_assign_vertex() {
    Vertex a = Vertex(vec3(1.0, 0.0, 0.0), vec3(0.0), 0.0, 0.0);
    Vertex b = Vertex(vec3(9.0, 0.0, 0.0), vec3(0.0), 2.0, 0.0);
    a = b;
    return a.position.x + a.u; // 9 + 2 = 11
}

// @unimplemented(jit.q32)
// run: test_mixed_assign_vertex() ~= 11.0

int test_mixed_assign_material_flags() {
    Material a = Material(vec3(0.0), vec3(0.0), 0.0, 1);
    Material b = Material(vec3(0.0), vec3(0.0), 0.0, 99);
    a = b;
    return a.flags;
}

// @unimplemented(jit.q32)
// run: test_mixed_assign_material_flags() == 99

float test_mixed_assign_scene_object_scale() {
    SceneObject a = SceneObject(vec3(0.0), mat3(1.0), 1.0, Material(vec3(0.0), vec3(0.0), 0.0, 0));
    SceneObject b = SceneObject(vec3(7.0, 0.0, 0.0), mat3(2.0), 0.5, Material(vec3(0.0), vec3(0.0), 0.0, 0));
    a = b;
    return a.position.x + a.scale; // 7.5
}

// @unimplemented(jit.q32)
// run: test_mixed_assign_scene_object_scale() ~= 7.5

// --- Complex expressions -----------------------------------------------------

float test_mixed_complex_expr() {
    SceneObject o = SceneObject(
        vec3(1.0),
        mat3(1.0),
        2.0,
        Material(vec3(0.1), vec3(0.5), 32.0, 1));
    return o.position.x * o.scale + o.mat.shininess; // 1.0 * 2.0 + 32.0 = 34.0
}

// @unimplemented(jit.q32)
// run: test_mixed_complex_expr() ~= 34.0

float test_mixed_complex_dot_and_flags() {
    Vertex v = Vertex(vec3(1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 0.0, 0.0);
    Material m = Material(vec3(0.0), vec3(0.0), 0.0, 4);
    return dot(v.position, v.normal) + float(m.flags); // 1 + 4 = 5
}

// @unimplemented(jit.q32)
// run: test_mixed_complex_dot_and_flags() ~= 5.0

float test_mixed_complex_column_scale_diffuse() {
    SceneObject o = SceneObject(
        vec3(0.0),
        mat3(1.0),
        2.0,
        Material(vec3(0.0), vec3(0.5, 0.0, 0.0), 0.0, 0));
    return o.rotation[0].x * o.scale + o.mat.diffuse.x; // 1*2 + 0.5 = 2.5
}

// @unimplemented(jit.q32)
// run: test_mixed_complex_column_scale_diffuse() ~= 2.5

float test_mixed_game_two_objects_distance_hint() {
    SceneObject player = SceneObject(
        vec3(0.0, 0.0, 0.0),
        mat3(1.0),
        1.0,
        Material(vec3(0.0), vec3(0.0), 0.0, 0));
    SceneObject pickup = SceneObject(
        vec3(3.0, 4.0, 0.0),
        mat3(1.0),
        1.0,
        Material(vec3(0.0), vec3(0.0), 0.0, 0));
    vec3 d = pickup.position - player.position;
    return length(d); // 5.0
}

// @unimplemented(jit.q32)
// run: test_mixed_game_two_objects_distance_hint() ~= 5.0
