// test run
// target riscv32.q32

// ============================================================================
// Struct Return Types: User-defined structures
// ============================================================================

struct Point2D {
    float x;
    float y;
};

struct Color {
    vec3 rgb;
    float alpha;
};

struct Triangle {
    Point2D a;
    Point2D b;
    Point2D c;
};

Point2D get_origin() {
    return Point2D(0.0, 0.0);
}

Point2D test_return_struct_simple() {
    // Return simple struct
    return get_origin();
}

// run: test_return_struct_simple() ~= Point2D(0.0, 0.0) [expect-fail]

Color get_red() {
    return Color(vec3(1.0, 0.0, 0.0), 1.0);
}

Color test_return_struct_color() {
    // Return color struct
    return get_red();
}

// run: test_return_struct_color() ~= Color(vec3(1.0, 0.0, 0.0), 1.0) [expect-fail]

Point2D add_points(Point2D p1, Point2D p2) {
    return Point2D(p1.x + p2.x, p1.y + p2.y);
}

Point2D test_return_struct_calculated() {
    // Return calculated struct values
    Point2D a = Point2D(1.0, 2.0);
    Point2D b = Point2D(3.0, 4.0);
    return add_points(a, b);
}

// run: test_return_struct_calculated() ~= Point2D(4.0, 6.0) [expect-fail]

Color blend_colors(Color c1, Color c2, float factor) {
    vec3 blended_rgb = mix(c1.rgb, c2.rgb, factor);
    float blended_alpha = mix(c1.alpha, c2.alpha, factor);
    return Color(blended_rgb, blended_alpha);
}

Color test_return_struct_mixed() {
    // Return struct with mixed operations
    Color red = Color(vec3(1.0, 0.0, 0.0), 1.0);
    Color blue = Color(vec3(0.0, 0.0, 1.0), 0.8);
    return blend_colors(red, blue, 0.5);
}

// run: test_return_struct_mixed() ~= Color(vec3(0.5, 0.0, 0.5), 0.9) [expect-fail]

Triangle get_equilateral_triangle(float side) {
    float height = side * 0.866; // sqrt(3)/2
    return Triangle(
        Point2D(0.0, 0.0),
        Point2D(side, 0.0),
        Point2D(side * 0.5, height)
    );
}

Triangle test_return_struct_nested() {
    // Return struct with nested structs
    return get_equilateral_triangle(2.0);
}

// run: test_return_struct_nested() ~= Triangle(Point2D(0.0, 0.0), Point2D(2.0, 0.0), Point2D(1.0, 1.732)) [expect-fail]

Point2D scale_point(Point2D p, float scale) {
    return Point2D(p.x * scale, p.y * scale);
}

Point2D test_return_struct_modified() {
    // Return modified struct
    Point2D original = Point2D(3.0, 4.0);
    return scale_point(original, 2.0);
}

// run: test_return_struct_modified() ~= Point2D(6.0, 8.0) [expect-fail]

Color make_color(float r, float g, float b) {
    return Color(vec3(r, g, b), 1.0);
}

Color test_return_struct_constructor() {
    // Return struct using constructor syntax
    return make_color(0.5, 0.7, 0.9);
}

// run: test_return_struct_constructor() ~= Color(vec3(0.5, 0.7, 0.9), 1.0) [expect-fail]

struct Vector3D {
    float x, y, z;
};

Vector3D get_up_vector() {
    return Vector3D(0.0, 1.0, 0.0);
}

Vector3D test_return_struct_compact() {
    // Return compact struct definition
    return get_up_vector();
}

// run: test_return_struct_compact() ~= Vector3D(0.0, 1.0, 0.0) [expect-fail]
