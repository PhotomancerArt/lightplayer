// test run
// target riscv32.q32

// ============================================================================
// Struct Parameters: User-defined structures as parameters
// ============================================================================

struct Point {
    float x, y;
};

struct Circle {
    Point center;
    float radius;
};

struct Color {
    vec3 rgb;
    float alpha;
};

float distance_from_origin(Point p) {
    return sqrt(p.x * p.x + p.y * p.y);
}

float test_param_struct_simple() {
    // Simple struct parameter
    Point origin = Point(3.0, 4.0);
    return distance_from_origin(origin);
}

// run: test_param_struct_simple() ~= 5.0 [expect-fail]

void move_point(inout Point p, float dx, float dy) {
    p.x = p.x + dx;
    p.y = p.y + dy;
}

void test_param_struct_modify() {
    // Modify struct fields
    Point p = Point(1.0, 2.0);
    move_point(p, 5.0, 3.0);
    // p should now be (6.0, 5.0)
}

// run: test_param_struct_modify() == 0.0 [expect-fail]

float circle_area(Circle c) {
    return 3.14159 * c.radius * c.radius;
}

float test_param_struct_nested() {
    // Nested struct parameters
    Circle circle = Circle(Point(0.0, 0.0), 2.0);
    return circle_area(circle);
}

// run: test_param_struct_nested() ~= 12.56636 [expect-fail]

Color blend_colors(Color c1, Color c2, float factor) {
    return Color(mix(c1.rgb, c2.rgb, factor), mix(c1.alpha, c2.alpha, factor));
}

Color test_param_struct_return() {
    // Struct parameters and return values
    Color red = Color(vec3(1.0, 0.0, 0.0), 1.0);
    Color blue = Color(vec3(0.0, 0.0, 1.0), 0.8);
    return blend_colors(red, blue, 0.5);
}

// run: test_param_struct_return() ~= Color(vec3(0.5, 0.0, 0.5), 0.9) [expect-fail]

void create_circle(out Circle c, Point center, float radius) {
    c.center = center;
    c.radius = radius;
}

void test_param_struct_out() {
    // Out struct parameters
    Circle circle;
    create_circle(circle, Point(1.0, 2.0), 5.0);
    // circle should be properly initialized
}

// run: test_param_struct_out() == 0.0 [expect-fail]

float get_alpha(const Color c) {
    return c.alpha;
}

float test_param_struct_const() {
    // Const struct parameters
    Color color = Color(vec3(0.5, 0.5, 0.5), 0.7);
    return get_alpha(color);
}

// run: test_param_struct_const() ~= 0.7 [expect-fail]

void process_circle(in Circle input, out Circle output, inout Point center) {
    output = input;
    output.radius = output.radius * 2.0;
    center.x = center.x + 10.0;
    center.y = center.y + 10.0;
}

float test_param_struct_mixed_qualifiers() {
    // Mixed qualifiers with structs
    Circle in_circle = Circle(Point(0.0, 0.0), 3.0);
    Circle out_circle;
    Point center = Point(1.0, 1.0);
    process_circle(in_circle, out_circle, center);
    return out_circle.radius + center.x + center.y; // 6.0 + 11.0 + 11.0 = 28.0
}

// run: test_param_struct_mixed_qualifiers() ~= 28.0 [expect-fail]

struct Triangle {
    Point a, b, c;
};

float triangle_perimeter(Triangle t) {
    float dist_ab = distance(vec2(t.a.x, t.a.y), vec2(t.b.x, t.b.y));
    float dist_bc = distance(vec2(t.b.x, t.b.y), vec2(t.c.x, t.c.y));
    float dist_ca = distance(vec2(t.c.x, t.c.y), vec2(t.a.x, t.a.y));
    return dist_ab + dist_bc + dist_ca;
}

float test_param_struct_complex() {
    // Complex struct with multiple fields
    Triangle triangle = Triangle(
        Point(0.0, 0.0),
        Point(3.0, 0.0),
        Point(0.0, 4.0)
    );
    return triangle_perimeter(triangle);
}

// run: test_param_struct_complex() ~= 12.0 [expect-fail]
