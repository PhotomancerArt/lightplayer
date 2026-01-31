// test run
// target riscv32.q32

// ============================================================================
// Struct Constructors with Vector Members
// ============================================================================

struct Transform {
    vec3 position;
    vec3 rotation;
};

vec3 test_constructor_vectors_transform_position() {
    Transform t = Transform(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    return t.position; // Should be vec3(1.0, 2.0, 3.0)
}

// run: test_constructor_vectors_transform_position() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

vec3 test_constructor_vectors_transform_rotation() {
    Transform t = Transform(vec3(10.0, 20.0, 30.0), vec3(0.1, 0.2, 0.3));
    return t.rotation; // Should be vec3(0.1, 0.2, 0.3)
}

// run: test_constructor_vectors_transform_rotation() ~= vec3(0.1, 0.2, 0.3) [expect-fail]

struct ColorRGBA {
    vec4 rgba;
};

vec4 test_constructor_vectors_color_rgba() {
    ColorRGBA c = ColorRGBA(vec4(0.1, 0.2, 0.3, 0.4));
    return c.rgba; // Should be vec4(0.1, 0.2, 0.3, 0.4)
}

// run: test_constructor_vectors_color_rgba() ~= vec4(0.1, 0.2, 0.3, 0.4) [expect-fail]

struct LineSegment {
    vec2 start;
    vec2 end;
};

vec2 test_constructor_vectors_line_segment() {
    LineSegment l = LineSegment(vec2(0.0, 0.0), vec2(10.0, 10.0));
    return l.start; // Should be vec2(0.0, 0.0)
}

// run: test_constructor_vectors_line_segment() ~= vec2(0.0, 0.0) [expect-fail]

vec2 test_constructor_vectors_line_segment_end() {
    LineSegment l = LineSegment(vec2(5.0, 5.0), vec2(15.0, 15.0));
    return l.end; // Should be vec2(15.0, 15.0)
}

// run: test_constructor_vectors_line_segment_end() ~= vec2(15.0, 15.0) [expect-fail]

struct Triangle3D {
    vec3 v1;
    vec3 v2;
    vec3 v3;
};

vec3 test_constructor_vectors_triangle3d() {
    Triangle3D t = Triangle3D(vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0));
    return t.v2; // Should be vec3(1.0, 0.0, 0.0)
}

// run: test_constructor_vectors_triangle3d() ~= vec3(1.0, 0.0, 0.0) [expect-fail]

struct MatrixTransform {
    vec4 row0;
    vec4 row1;
    vec4 row2;
    vec4 row3;
};

vec4 test_constructor_vectors_matrix_transform() {
    MatrixTransform m = MatrixTransform(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(0.0, 0.0, 0.0, 1.0)
    );
    return m.row2; // Should be vec4(0.0, 0.0, 1.0, 0.0)
}

// run: test_constructor_vectors_matrix_transform() ~= vec4(0.0, 0.0, 1.0, 0.0) [expect-fail]

struct Particle {
    vec3 position;
    vec3 velocity;
    vec4 color;
    float size;
};

vec3 test_constructor_vectors_particle_position() {
    Particle p = Particle(vec3(1.0, 2.0, 3.0), vec3(0.1, 0.2, 0.3), vec4(1.0, 1.0, 1.0, 1.0), 5.0);
    return p.position; // Should be vec3(1.0, 2.0, 3.0)
}

// run: test_constructor_vectors_particle_position() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

vec4 test_constructor_vectors_particle_color() {
    Particle p = Particle(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0), vec4(0.5, 0.5, 0.5, 0.8), 2.0);
    return p.color; // Should be vec4(0.5, 0.5, 0.5, 0.8)
}

// run: test_constructor_vectors_particle_color() ~= vec4(0.5, 0.5, 0.5, 0.8) [expect-fail]

float test_constructor_vectors_particle_size() {
    Particle p = Particle(vec3(10.0, 20.0, 30.0), vec3(0.0, 0.0, 0.0), vec4(0.0, 0.0, 0.0, 0.0), 15.5);
    return p.size; // Should be 15.5
}

// run: test_constructor_vectors_particle_size() ~= 15.5 [expect-fail]
