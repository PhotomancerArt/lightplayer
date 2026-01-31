// test run
// target riscv32.q32

// ============================================================================
// Struct Definitions with Vector Members
// ============================================================================

struct Transform {
    vec3 position;
    vec3 rotation;
};

float test_define_vector_transform() {
    Transform t; // Declaration test
    return 1.0; // Should be 1.0
}

// run: test_define_vector_transform() == 1.0 [expect-fail]

struct ColorRGBA {
    vec4 rgba;
};

int test_define_vector_color_rgba() {
    ColorRGBA c; // Declaration test
    return 1; // Should be 1
}

// run: test_define_vector_color_rgba() == 1 [expect-fail]

struct LineSegment {
    vec2 start;
    vec2 end;
};

uint test_define_vector_line_segment() {
    LineSegment l; // Declaration test
    return 1u; // Should be 1u
}

// run: test_define_vector_line_segment() == 1u [expect-fail]

struct Triangle3D {
    vec3 v1;
    vec3 v2;
    vec3 v3;
};

bool test_define_vector_triangle3d() {
    Triangle3D t; // Declaration test
    return true; // Should be true
}

// run: test_define_vector_triangle3d() == true [expect-fail]

struct MatrixTransform {
    vec4 row0;
    vec4 row1;
    vec4 row2;
    vec4 row3;
};

vec2 test_define_vector_matrix_transform() {
    MatrixTransform m; // Declaration test
    return vec2(1.0, 1.0); // Should be vec2(1.0, 1.0)
}

// run: test_define_vector_matrix_transform() ~= vec2(1.0, 1.0) [expect-fail]

struct Particle {
    vec3 position;
    vec3 velocity;
    vec4 color;
    float size;
};

float test_define_vector_particle() {
    Particle p; // Declaration test
    return 1.0; // Should be 1.0
}

// run: test_define_vector_particle() == 1.0 [expect-fail]

struct BoundingBox {
    vec2 min;
    vec2 max;
};

int test_define_vector_bounding_box() {
    BoundingBox b; // Declaration test
    return 1; // Should be 1
}

// run: test_define_vector_bounding_box() == 1 [expect-fail]

struct Light {
    vec3 position;
    vec3 direction;
    vec3 color;
    float intensity;
};

uint test_define_vector_light() {
    Light l; // Declaration test
    return 1u; // Should be 1u
}

// run: test_define_vector_light() == 1u [expect-fail]
