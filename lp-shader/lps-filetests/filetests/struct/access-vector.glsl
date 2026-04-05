// test run

// ============================================================================
// Vector Member Access
// ============================================================================

struct Transform {
    vec3 position;
    vec3 rotation;
};

vec3 test_access_vector_transform_position() {
    Transform t = Transform(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    return t.position; // Access position member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_transform_position() ~= vec3(1.0, 2.0, 3.0)

vec3 test_access_vector_transform_rotation() {
    Transform t = Transform(vec3(10.0, 20.0, 30.0), vec3(0.1, 0.2, 0.3));
    return t.rotation; // Access rotation member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_transform_rotation() ~= vec3(0.1, 0.2, 0.3)

float test_access_vector_transform_position_x() {
    Transform t = Transform(vec3(5.0, 6.0, 7.0), vec3(0.0, 0.0, 0.0));
    return t.position.x; // Access vector component
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_transform_position_x() ~= 5.0

struct ColorRGBA {
    vec4 rgba;
};

vec4 test_access_vector_color_rgba() {
    ColorRGBA c = ColorRGBA(vec4(0.1, 0.2, 0.3, 0.4));
    return c.rgba; // Access rgba member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_color_rgba() ~= vec4(0.1, 0.2, 0.3, 0.4)

float test_access_vector_color_rgba_alpha() {
    ColorRGBA c = ColorRGBA(vec4(1.0, 0.5, 0.0, 0.8));
    return c.rgba.w; // Access vector component (alpha)
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_color_rgba_alpha() ~= 0.8

struct LineSegment {
    vec2 start;
    vec2 end;
};

vec2 test_access_vector_line_segment_start() {
    LineSegment l = LineSegment(vec2(0.0, 0.0), vec2(10.0, 10.0));
    return l.start; // Access start member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_line_segment_start() ~= vec2(0.0, 0.0)

vec2 test_access_vector_line_segment_end() {
    LineSegment l = LineSegment(vec2(5.0, 5.0), vec2(15.0, 15.0));
    return l.end; // Access end member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_line_segment_end() ~= vec2(15.0, 15.0)

float test_access_vector_line_segment_start_y() {
    LineSegment l = LineSegment(vec2(1.0, 2.0), vec2(3.0, 4.0));
    return l.start.y; // Access vector component
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_line_segment_start_y() ~= 2.0

struct Triangle3D {
    vec3 v1;
    vec3 v2;
    vec3 v3;
};

vec3 test_access_vector_triangle3d_v2() {
    Triangle3D t = Triangle3D(vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0));
    return t.v2; // Access v2 member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_triangle3d_v2() ~= vec3(1.0, 0.0, 0.0)

float test_access_vector_triangle3d_v3_z() {
    Triangle3D t = Triangle3D(vec3(1.0, 1.0, 1.0), vec3(2.0, 2.0, 2.0), vec3(3.0, 3.0, 3.0));
    return t.v3.z; // Access vector component
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_triangle3d_v3_z() ~= 3.0

struct Particle {
    vec3 position;
    vec3 velocity;
    vec4 color;
    float size;
};

vec3 test_access_vector_particle_velocity() {
    Particle p = Particle(vec3(1.0, 2.0, 3.0), vec3(0.1, 0.2, 0.3), vec4(1.0, 1.0, 1.0, 1.0), 5.0);
    return p.velocity; // Access velocity member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_particle_velocity() ~= vec3(0.1, 0.2, 0.3)

vec4 test_access_vector_particle_color() {
    Particle p = Particle(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0), vec4(0.5, 0.5, 0.5, 0.8), 2.0);
    return p.color; // Access color member
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// run: test_access_vector_particle_color() ~= vec4(0.5, 0.5, 0.5, 0.8)
