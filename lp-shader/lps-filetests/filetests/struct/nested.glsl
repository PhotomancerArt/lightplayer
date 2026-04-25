// test run

// ============================================================================
// One-level nested structs: scalar path (Line, Material, Circle) and vector
// path (VPair: nested structs whose leaves are vec2).
// ============================================================================

struct Point {
    float x;
    float y;
};

struct Color {
    float r;
    float g;
    float b;
};

struct Line {
    Point start;
    Point end;
};

struct Material {
    Color diffuse;
    Color specular;
    float shininess;
};

struct Circle {
    Point center;
    float radius;
};

// Nested members use vec2 for struct.nested.vector lowering coverage.
struct VPoint {
    vec2 p;
};

struct VPair {
    VPoint a;
    VPoint b;
};

// ----------------------------------------------------------------------------
// 1. Declaration
// ----------------------------------------------------------------------------

float test_nested_declare_line() {
    Line l;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_nested_declare_line() ~= 1.0

float test_nested_declare_material() {
    Material m;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_nested_declare_material() ~= 1.0

float test_nested_declare_circle() {
    Circle c;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_nested_declare_circle() ~= 1.0

// ----------------------------------------------------------------------------
// 2. Construction
// ----------------------------------------------------------------------------

float test_nested_construct_line_start_x() {
    Line l = Line(Point(1.0, 2.0), Point(3.0, 4.0));
    return l.start.x;
}

// @unimplemented(jit.q32)
// run: test_nested_construct_line_start_x() ~= 1.0

float test_nested_construct_line_end_y() {
    Line l = Line(Point(5.0, 6.0), Point(7.0, 8.0));
    return l.end.y;
}

// @unimplemented(jit.q32)
// run: test_nested_construct_line_end_y() ~= 8.0

float test_nested_construct_material_diffuse_r() {
    Material m = Material(Color(0.1, 0.2, 0.3), Color(0.8, 0.9, 1.0), 32.0);
    return m.diffuse.r;
}

// @unimplemented(jit.q32)
// run: test_nested_construct_material_diffuse_r() ~= 0.1

float test_nested_construct_material_shininess() {
    Material m = Material(Color(1.0, 0.0, 0.0), Color(0.0, 1.0, 0.0), 64.0);
    return m.shininess;
}

// @unimplemented(jit.q32)
// run: test_nested_construct_material_shininess() ~= 64.0

float test_nested_construct_circle() {
    Circle c = Circle(Point(2.0, 3.0), 5.0);
    return c.center.x + c.radius;
}

// @unimplemented(jit.q32)
// run: test_nested_construct_circle() ~= 7.0

// ----------------------------------------------------------------------------
// 3. Deep member read
// ----------------------------------------------------------------------------

float test_nested_deep_read_line_end_x() {
    Line l = Line(Point(0.0, 0.0), Point(9.0, 1.0));
    return l.end.x;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_read_line_end_x() ~= 9.0

float test_nested_deep_read_material_specular_b() {
    Material m = Material(Color(0.0, 0.0, 0.0), Color(0.4, 0.5, 0.6), 0.0);
    return m.specular.b;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_read_material_specular_b() ~= 0.6

float test_nested_deep_read_circle_center_y() {
    Circle c = Circle(Point(1.0, 2.5), 0.0);
    return c.center.y;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_read_circle_center_y() ~= 2.5

// ----------------------------------------------------------------------------
// 4. Deep member write
// ----------------------------------------------------------------------------

float test_nested_deep_write_line_start_x() {
    Line l = Line(Point(0.0, 0.0), Point(0.0, 0.0));
    l.start.x = 5.0;
    return l.start.x;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_write_line_start_x() ~= 5.0

float test_nested_deep_write_material_shininess() {
    Material m = Material(Color(0.0, 0.0, 0.0), Color(0.0, 0.0, 0.0), 0.0);
    m.shininess = 128.0;
    return m.shininess;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_write_material_shininess() ~= 128.0

float test_nested_deep_write_material_diffuse_r() {
    Material m = Material(Color(0.0, 0.0, 0.0), Color(0.0, 0.0, 0.0), 0.0);
    m.diffuse.r = 0.25;
    return m.diffuse.r;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_write_material_diffuse_r() ~= 0.25

float test_nested_deep_write_circle_radius() {
    Circle c = Circle(Point(0.0, 0.0), 1.0);
    c.radius = 10.0;
    return c.radius;
}

// @unimplemented(jit.q32)
// run: test_nested_deep_write_circle_radius() ~= 10.0

// ----------------------------------------------------------------------------
// 5. Whole-struct assignment
// ----------------------------------------------------------------------------

float test_nested_whole_assign_line() {
    Line l1 = Line(Point(1.0, 2.0), Point(3.0, 4.0));
    Line l2 = Line(Point(5.0, 6.0), Point(7.0, 8.0));
    l1 = l2;
    return l1.start.x;
}

// @unimplemented(jit.q32)
// run: test_nested_whole_assign_line() ~= 5.0

float test_nested_whole_assign_material() {
    Material m1 = Material(Color(0.1, 0.0, 0.0), Color(0.0, 0.0, 0.0), 0.0);
    Material m2 = Material(Color(0.9, 0.0, 0.0), Color(0.0, 0.0, 0.0), 0.0);
    m1 = m2;
    return m1.diffuse.r;
}

// @unimplemented(jit.q32)
// run: test_nested_whole_assign_material() ~= 0.9

float test_nested_whole_assign_circle() {
    Circle c1 = Circle(Point(0.0, 0.0), 0.0);
    Circle c2 = Circle(Point(3.0, 4.0), 0.0);
    c1 = c2;
    return c1.center.x + c1.center.y;
}

// @unimplemented(jit.q32)
// run: test_nested_whole_assign_circle() ~= 7.0

// ----------------------------------------------------------------------------
// 6. Mixed: construct, modify nested, read back
// ----------------------------------------------------------------------------

float test_nested_mixed_line() {
    Line l = Line(Point(1.0, 1.0), Point(2.0, 2.0));
    l.start.x = 3.0;
    l.end.y = 4.0;
    return l.start.x + l.end.y;
}

// @unimplemented(jit.q32)
// run: test_nested_mixed_line() ~= 7.0

float test_nested_mixed_material() {
    Material m = Material(Color(0.1, 0.2, 0.3), Color(0.0, 0.0, 0.0), 4.0);
    m.shininess = 8.0;
    m.specular.r = 1.0;
    return m.diffuse.g + m.specular.r;
}

// @unimplemented(jit.q32)
// run: test_nested_mixed_material() ~= 1.2

float test_nested_mixed_circle() {
    Circle c = Circle(Point(0.0, 0.0), 1.0);
    c.center.x = 2.0;
    c.center.y = 3.0;
    c.radius = c.center.x * 2.0;
    return c.radius;
}

// @unimplemented(jit.q32)
// run: test_nested_mixed_circle() ~= 4.0

// ----------------------------------------------------------------------------
// 7. Nested vector path (VPair: inner VPoint holds vec2)
// ----------------------------------------------------------------------------

vec2 test_nested_vpair_construct_a() {
    VPair p = VPair(VPoint(vec2(1.0, 2.0)), VPoint(vec2(3.0, 4.0)));
    return p.a.p;
}

// @unimplemented(jit.q32)
// run: test_nested_vpair_construct_a() ~= vec2(1.0, 2.0)

float test_nested_vpair_deep_read_b_x() {
    VPair p = VPair(VPoint(vec2(0.0, 0.0)), VPoint(vec2(5.0, 6.0)));
    return p.b.p.x;
}

// @unimplemented(jit.q32)
// run: test_nested_vpair_deep_read_b_x() ~= 5.0

float test_nested_vpair_deep_write() {
    VPair p = VPair(VPoint(vec2(0.0, 0.0)), VPoint(vec2(0.0, 0.0)));
    p.a.p = vec2(7.0, 8.0);
    return p.a.p.x + p.a.p.y;
}

// @unimplemented(jit.q32)
// run: test_nested_vpair_deep_write() ~= 15.0

vec2 test_nested_vpair_whole_assign() {
    VPair p1 = VPair(VPoint(vec2(1.0, 1.0)), VPoint(vec2(1.0, 1.0)));
    VPair p2 = VPair(VPoint(vec2(2.0, 3.0)), VPoint(vec2(4.0, 5.0)));
    p1 = p2;
    return p1.a.p;
}

// @unimplemented(jit.q32)
// run: test_nested_vpair_whole_assign() ~= vec2(2.0, 3.0)
