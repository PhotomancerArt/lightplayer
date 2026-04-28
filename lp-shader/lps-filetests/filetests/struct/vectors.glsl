// test run

// ============================================================================
// Structs with vector members (vec2, vec3, vec4) — all operations
// ============================================================================

struct Transform {
    vec3 position;
    vec3 rotation;
};

struct ColorRGBA {
    vec4 rgba;
};

struct LineSegment {
    vec2 start;
    vec2 end;
};

struct Particle {
    vec3 pos;
    vec3 vel;
    vec4 color;
    float size;
};

// --- Declaration: locals with vec2/vec3/vec4 members

float test_vectors_decl_transform_sum_xy() {
    Transform t = Transform(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    return t.position.x + t.rotation.y; // 1 + 5
}

// @unimplemented(jit.q32)
// run: test_vectors_decl_transform_sum_xy() ~= 6.0

float test_vectors_decl_line_spans_axis() {
    LineSegment s = LineSegment(vec2(0.0, 0.0), vec2(3.0, 4.0));
    return s.end.x - s.start.x; // 3.0
}

// @unimplemented(jit.q32)
// run: test_vectors_decl_line_spans_axis() ~= 3.0

// --- Construction: vector initialization in constructors

float test_vectors_construct_transform() {
    Transform t = Transform(vec3(1.0, 2.0, 3.0), vec3(0.0));
    return t.position.x;
}

// @unimplemented(jit.q32)
// run: test_vectors_construct_transform() ~= 1.0

vec4 test_vectors_construct_color() {
    ColorRGBA c = ColorRGBA(vec4(0.1, 0.2, 0.3, 0.4));
    return c.rgba;
}

// @unimplemented(jit.q32)
// run: test_vectors_construct_color() ~= vec4(0.1, 0.2, 0.3, 0.4)

vec2 test_vectors_construct_line() {
    LineSegment l = LineSegment(vec2(7.0, 8.0), vec2(9.0, 10.0));
    return l.start;
}

// @unimplemented(jit.q32)
// run: test_vectors_construct_line() ~= vec2(7.0, 8.0)

float test_vectors_construct_particle_size() {
    Particle p = Particle(
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec4(1.0, 0.0, 0.0, 1.0),
        2.5
    );
    return p.size;
}

// @unimplemented(jit.q32)
// run: test_vectors_construct_particle_size() ~= 2.5

// --- Member read: whole vectors and scalars

vec3 test_vectors_read_transform_rotation() {
    Transform t = Transform(vec3(0.0, 0.0, 0.0), vec3(0.1, 0.2, 0.3));
    return t.rotation;
}

// @unimplemented(jit.q32)
// run: test_vectors_read_transform_rotation() ~= vec3(0.1, 0.2, 0.3)

float test_vectors_read_color_rgba_b() {
    ColorRGBA c = ColorRGBA(vec4(1.0, 0.5, 0.25, 0.0));
    return c.rgba.y; // 0.5
}

// @unimplemented(jit.q32)
// run: test_vectors_read_color_rgba_b() ~= 0.5

float test_vectors_read_line_end_x() {
    LineSegment l = LineSegment(vec2(1.0, 1.0), vec2(11.0, 12.0));
    return l.end.x;
}

// @unimplemented(jit.q32)
// run: test_vectors_read_line_end_x() ~= 11.0

vec3 test_vectors_read_particle_pos() {
    Particle p = Particle(
        vec3(10.0, 20.0, 30.0),
        vec3(0.0, 0.0, 0.0),
        vec4(0.0, 0.0, 0.0, 0.0),
        0.0
    );
    return p.pos;
}

// @unimplemented(jit.q32)
// run: test_vectors_read_particle_pos() ~= vec3(10.0, 20.0, 30.0)

// --- Member write: assign into vector and scalar members

float test_vectors_write_transform_position_x() {
    Transform t = Transform(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0));
    t.position = vec3(9.0, 0.0, 0.0);
    return t.position.x;
}

// @unimplemented(jit.q32)
// run: test_vectors_write_transform_position_x() ~= 9.0

vec4 test_vectors_write_color_rgba() {
    ColorRGBA c = ColorRGBA(vec4(0.0, 0.0, 0.0, 0.0));
    c.rgba = vec4(0.0, 1.0, 0.0, 0.5);
    return c.rgba;
}

// @unimplemented(jit.q32)
// run: test_vectors_write_color_rgba() ~= vec4(0.0, 1.0, 0.0, 0.5)

float test_vectors_write_line_start_y() {
    LineSegment l = LineSegment(vec2(0.0, 0.0), vec2(0.0, 0.0));
    l.start = vec2(0.0, 6.0);
    return l.start.y;
}

// @unimplemented(jit.q32)
// run: test_vectors_write_line_start_y() ~= 6.0

float test_vectors_write_particle_size() {
    Particle p = Particle(
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec4(0.0, 0.0, 0.0, 0.0),
        0.0
    );
    p.size = 42.0;
    return p.size;
}

// @unimplemented(jit.q32)
// run: test_vectors_write_particle_size() ~= 42.0

// --- Whole-struct assignment: copy vec-containing structs

vec3 test_vectors_assign_transform_position() {
    Transform t1 = Transform(vec3(1.0, 2.0, 3.0), vec3(0.0, 0.0, 0.0));
    Transform t2 = Transform(vec3(7.0, 8.0, 9.0), vec3(0.0, 0.0, 0.0));
    t1 = t2;
    return t1.position;
}

// @unimplemented(jit.q32)
// run: test_vectors_assign_transform_position() ~= vec3(7.0, 8.0, 9.0)

float test_vectors_assign_color_rgba_w() {
    ColorRGBA a = ColorRGBA(vec4(0.0, 0.0, 0.0, 0.0));
    ColorRGBA b = ColorRGBA(vec4(0.0, 0.0, 0.0, 0.9));
    a = b;
    return a.rgba.w;
}

// @unimplemented(jit.q32)
// run: test_vectors_assign_color_rgba_w() ~= 0.9

vec2 test_vectors_assign_line_start() {
    LineSegment a = LineSegment(vec2(0.0, 0.0), vec2(0.0, 0.0));
    LineSegment b = LineSegment(vec2(2.0, 3.0), vec2(0.0, 0.0));
    a = b;
    return a.start;
}

// @unimplemented(jit.q32)
// run: test_vectors_assign_line_start() ~= vec2(2.0, 3.0)

vec3 test_vectors_assign_particle_vel() {
    Particle a = Particle(
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec4(0.0, 0.0, 0.0, 0.0),
        0.0
    );
    Particle b = Particle(
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 2.0),
        vec4(0.0, 0.0, 0.0, 0.0),
        0.0
    );
    a = b;
    return a.vel;
}

// @unimplemented(jit.q32)
// run: test_vectors_assign_particle_vel() ~= vec3(0.0, 1.0, 2.0)

// --- Read vector then swizzle (struct -> member -> swizzle)

vec2 test_vectors_swizzle_transform_position_xy() {
    Transform t = Transform(vec3(3.0, 4.0, 5.0), vec3(0.0, 0.0, 0.0));
    return t.position.xy;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_transform_position_xy() ~= vec2(3.0, 4.0)

float test_vectors_swizzle_transform_rotation_z() {
    Transform t = Transform(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.5));
    return t.rotation.z;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_transform_rotation_z() ~= 0.5

vec3 test_vectors_swizzle_color_rgba_rgb() {
    ColorRGBA c = ColorRGBA(vec4(1.0, 0.0, 0.0, 0.0));
    return c.rgba.rgb;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_color_rgba_rgb() ~= vec3(1.0, 0.0, 0.0)

vec2 test_vectors_swizzle_line_start_yx() {
    LineSegment l = LineSegment(vec2(1.0, 2.0), vec2(0.0, 0.0));
    return l.start.yx;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_line_start_yx() ~= vec2(2.0, 1.0)

vec2 test_vectors_swizzle_particle_pos_xy() {
    Particle p = Particle(
        vec3(8.0, 9.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec4(0.0, 0.0, 0.0, 0.0),
        0.0
    );
    return p.pos.xy;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_particle_pos_xy() ~= vec2(8.0, 9.0)

float test_vectors_swizzle_particle_color_a() {
    Particle p = Particle(
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 0.0, 0.0),
        vec4(0.0, 0.0, 0.0, 0.25),
        0.0
    );
    return p.color.a;
}

// @unimplemented(jit.q32)
// run: test_vectors_swizzle_particle_color_a() ~= 0.25
